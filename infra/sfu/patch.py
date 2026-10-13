"""Pinned LiveKit integration. Fail rather than silently skipping an upstream change."""
from pathlib import Path
path = Path("pkg/rtc/participant.go")
source = path.read_text()
anchor = '\tif len(p.pendingTracks) >= cMaxPendingTracks {'
assert source.count(anchor) == 1
guard = '''\tif reason := p.hypercastCheckPublication(req); reason != "" {
        p.sendRequestResponse(&livekit.RequestResponse{
            Reason: livekit.RequestResponse_LIMIT_EXCEEDED,
            Message: reason,
            Request: &livekit.RequestResponse_AddTrack{AddTrack: proto.CloneOf(req)},
        })
        p.pendingTracksLock.Unlock()
        return nil
    }

'''
path.write_text(source.replace(anchor, guard + anchor))
