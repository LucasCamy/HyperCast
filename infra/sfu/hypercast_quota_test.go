package rtc
import (
    "testing"
    "sync"
    "sync/atomic"
    "fmt"
    "github.com/livekit/protocol/livekit"
)
func TestHypercastQuota(t *testing.T) {
    screen := &livekit.AddTrackRequest{Type: livekit.TrackType_VIDEO, Source: livekit.TrackSource_SCREEN_SHARE}
    if hypercastQuota("hc-screen-4", 3, screen, nil) == "" { t.Fatal("fourth slot admitted") }
    if hypercastQuota("hc-member-0123456789abcdef0123456789abcdef", 2, screen, nil) != "" { t.Fatal("member refused") }
    if hypercastQuota("hc-screen-1", 3, screen, nil) != "" { t.Fatal("valid slot refused") }
    pending := []*livekit.TrackInfo{{Source: livekit.TrackSource_SCREEN_SHARE}}
    if hypercastQuota("hc-screen-1", 3, screen, pending) == "" { t.Fatal("duplicate/pending screen admitted") }
    preview := &livekit.AddTrackRequest{Type: livekit.TrackType_VIDEO, Source: livekit.TrackSource_UNKNOWN, Name:"screen-preview"}
    if hypercastQuota("hc-screen-1", 3, preview, pending) == "" { t.Fatal("unknown source bypassed screen quota") }
    audio := &livekit.AddTrackRequest{Type: livekit.TrackType_AUDIO, Source: livekit.TrackSource_SCREEN_SHARE_AUDIO}
    if hypercastQuota("hc-screen-1", 3, audio, nil) == "" { t.Fatal("unpaired audio admitted") }
    screenWithType := []*livekit.TrackInfo{{Type:livekit.TrackType_VIDEO, Source:livekit.TrackSource_SCREEN_SHARE}}
    if hypercastQuota("hc-screen-1", 3, audio, screenWithType) != "" { t.Fatal("paired audio refused") }
    screenWithType = append(screenWithType, &livekit.TrackInfo{Type:livekit.TrackType_AUDIO, Source:livekit.TrackSource_SCREEN_SHARE_AUDIO})
    if hypercastQuota("hc-screen-1", 3, audio, screenWithType) == "" { t.Fatal("duplicate audio admitted") }
    audio.Source = livekit.TrackSource_MICROPHONE
    if hypercastQuota("hc-screen-1", 3, audio, screenWithType) == "" { t.Fatal("microphone admitted") }
}

func TestHypercastConcurrentRoomReservations(t *testing.T) {
    q := hypercastReservations{rooms: make(map[string]map[hypercastPublication]bool)}
    var wg sync.WaitGroup
    var accepted atomic.Int32
    for i := 0; i < 12; i++ {
        wg.Add(1)
        go func(id int) { defer wg.Done(); if q.reserve("room", fmt.Sprint(id), livekit.TrackSource_SCREEN_SHARE, 3) { accepted.Add(1) } }(i)
    }
    wg.Wait()
    if accepted.Load() != 3 { t.Fatalf("admitted %d", accepted.Load()) }
    if !q.reserve("other", "independent", livekit.TrackSource_SCREEN_SHARE, 1) { t.Fatal("rooms share quota") }
    q.release("other", "independent", livekit.TrackSource_SCREEN_SHARE)
    if !q.reserve("other", "next", livekit.TrackSource_SCREEN_SHARE, 1) { t.Fatal("unpublication did not free quota") }
}

func TestHypercastMixedVideoReservations(t *testing.T) {
    q := hypercastReservations{rooms: make(map[string]map[hypercastPublication]bool)}
    screen, camera := livekit.TrackSource_SCREEN_SHARE, livekit.TrackSource_CAMERA
    for _, id := range []string{"a", "b", "c"} {
        if !q.reserve("room", id, screen, 3) || !q.reserve("room", id, camera, 3) {
            t.Fatal("three screens and three cameras should fit")
        }
    }
    if q.reserve("room", "d", camera, 3) { t.Fatal("seventh video admitted") }
    q.release("room", "a", screen)
    if q.reserve("room", "a", camera, 3) { t.Fatal("duplicate camera admitted") }
    if !q.reserve("room", "d", camera, 3) { t.Fatal("camera could not use vacated screen slot") }
    q.release("room", "d", camera)
    if !q.reserve("room", "d", screen, 3) { t.Fatal("screen could not use vacated camera slot") }
}
