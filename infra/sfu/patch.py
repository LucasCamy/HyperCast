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

source = path.read_text()
anchor = '\tp.UpTrackManager.Close(isExpectedToResume)\n'
assert source.count(anchor) == 1
source = source.replace(anchor, anchor + '\thypercastRooms.release(p.ClaimGrants().Video.Room, string(p.ID()))\n')
anchor = '\t\tp.listener().OnTrackUnpublished(p, mt)\n'
assert source.count(anchor) == 1
source = source.replace(anchor, '\t\tif ti.Source == livekit.TrackSource_SCREEN_SHARE { hypercastRooms.release(quotaRoom, quotaParticipant) }\n' + anchor)
anchor = '\ttrackID := livekit.TrackID(ti.Sid)\n\tmt.AddOnClose('
assert source.count(anchor) == 1
source = source.replace(anchor, '\tquotaRoom, quotaParticipant := p.ClaimGrants().Video.Room, string(p.ID())\n' + anchor)
path.write_text(source)
