package rtc

import (
    "os"
    "strconv"
    "fmt"
    "strings"
    "sync"
    "github.com/livekit/protocol/livekit"
)

var hypercastLimit = func() int {
    raw := os.Getenv("HYPERCAST_MAX_PUBLISHERS")
    if raw == "" { return 3 }
    value, err := strconv.Atoi(raw)
    if err != nil || value < 1 || value > 3 { panic("HYPERCAST_MAX_PUBLISHERS must be 1..3") }
    return value
}()

type hypercastReservations struct {
    sync.Mutex
    rooms map[string]map[string]bool
}
var hypercastRooms = hypercastReservations{rooms: make(map[string]map[string]bool)}

func (q *hypercastReservations) reserve(room, participant string, limit int) bool {
    q.Lock()
    defer q.Unlock()
    members := q.rooms[room]
    if members == nil { members = make(map[string]bool); q.rooms[room] = members }
    if members[participant] || len(members) >= limit { return false }
    members[participant] = true
    return true
}
func (q *hypercastReservations) release(room, participant string) {
    q.Lock()
    defer q.Unlock()
    delete(q.rooms[room], participant)
    if len(q.rooms[room]) == 0 { delete(q.rooms, room) }
}

func hypercastAllowedIdentity(identity string, _ int) bool {
    for slot := 1; slot <= 3; slot++ {
        if identity == fmt.Sprintf("hc-screen-%d", slot) { return true }
    }
    id := strings.TrimPrefix(identity, "hc-member-")
    if id == identity || len(id) != 32 { return false }
    for _, char := range id { if !(char >= '0' && char <= '9' || char >= 'a' && char <= 'f') { return false } }
    return true
}

func hypercastQuota(identity string, limit int, req *livekit.AddTrackRequest, existing []*livekit.TrackInfo) string {
    if !hypercastAllowedIdentity(identity, limit) { return "No publisher slot assigned" }
    if req.Type != livekit.TrackType_VIDEO { return "Only screen video is permitted" }
    if req.Source != livekit.TrackSource_SCREEN_SHARE {
        return "Only screen_share video may be published"
    }
    for _, track := range existing {
        if track.Source == req.Source { return "This publisher slot already has this screen track" }
    }
    return ""
}

// Participant pending lock serializes duplicates; room mutex serializes all
// participants BEFORE publication ACK. Room membership does not consume quota.
func (p *ParticipantImpl) hypercastCheckPublication(req *livekit.AddTrackRequest) string {
    var tracks []*livekit.TrackInfo
    for _, track := range p.GetPublishedTracks() { tracks = append(tracks, track.ToProto()) }
    for _, pending := range p.pendingTracks { tracks = append(tracks, pending.trackInfos...) }
    if reason := hypercastQuota(string(p.Identity()), hypercastLimit, req, tracks); reason != "" { return reason }
    if p.IsClosed() { return "Participant is closed" }
    if !hypercastRooms.reserve(p.ClaimGrants().Video.Room, string(p.ID()), hypercastLimit) {
        return "Room screen publication limit reached"
    }
    return ""
}
