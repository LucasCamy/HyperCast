package rtc

import (
    "os"
    "strconv"
    "fmt"
    "github.com/livekit/protocol/livekit"
)

var hypercastLimit = func() int {
    raw := os.Getenv("HYPERCAST_MAX_PUBLISHERS")
    if raw == "" { return 3 }
    value, err := strconv.Atoi(raw)
    if err != nil || value < 1 || value > 3 { panic("HYPERCAST_MAX_PUBLISHERS must be 1..3") }
    return value
}()

func hypercastAllowedIdentity(identity string, limit int) bool {
    for slot := 1; slot <= limit; slot++ {
        if identity == fmt.Sprintf("hc-screen-%d", slot) { return true }
    }
    return false
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

// Called while pendingTracksLock is held, before a new track is acknowledged.
// Pending and active publications both count: simultaneous AddTrack requests
// cannot sneak a second screen through a check based only on active RTP tracks.
func (p *ParticipantImpl) hypercastCheckPublication(req *livekit.AddTrackRequest) string {
    var tracks []*livekit.TrackInfo
    for _, track := range p.GetPublishedTracks() { tracks = append(tracks, track.ToProto()) }
    for _, pending := range p.pendingTracks { tracks = append(tracks, pending.trackInfos...) }
    return hypercastQuota(string(p.Identity()), hypercastLimit, req, tracks)
}
