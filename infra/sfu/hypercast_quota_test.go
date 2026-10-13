package rtc
import (
    "testing"
    "github.com/livekit/protocol/livekit"
)
func TestHypercastQuota(t *testing.T) {
    screen := &livekit.AddTrackRequest{Type: livekit.TrackType_VIDEO, Source: livekit.TrackSource_SCREEN_SHARE}
    if hypercastQuota("hc-screen-4", 3, screen, nil) == "" { t.Fatal("fourth slot admitted") }
    if hypercastQuota("hc-screen-3", 2, screen, nil) == "" { t.Fatal("configured cap ignored") }
    if hypercastQuota("hc-screen-1", 3, screen, nil) != "" { t.Fatal("valid slot refused") }
    pending := []*livekit.TrackInfo{{Source: livekit.TrackSource_SCREEN_SHARE}}
    if hypercastQuota("hc-screen-1", 3, screen, pending) == "" { t.Fatal("duplicate/pending screen admitted") }
    preview := &livekit.AddTrackRequest{Type: livekit.TrackType_VIDEO, Source: livekit.TrackSource_UNKNOWN, Name:"screen-preview"}
    if hypercastQuota("hc-screen-1", 3, preview, pending) == "" { t.Fatal("unknown source bypassed screen quota") }
}
