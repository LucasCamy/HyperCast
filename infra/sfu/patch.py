"""Pinned LiveKit integration. Fail rather than silently skipping an upstream change."""
import json
import shutil
import subprocess
from pathlib import Path
path = Path("pkg/rtc/participant.go")
source = path.read_text()
anchor = '\tbackupCodecPolicy := req.BackupCodecPolicy'
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
source = source.replace(anchor, anchor + '\thypercastRooms.release(p.ClaimGrants().Video.Room, string(p.ID()), livekit.TrackSource_SCREEN_SHARE)\n\thypercastRooms.release(p.ClaimGrants().Video.Room, string(p.ID()), livekit.TrackSource_CAMERA)\n')
anchor = '\t\tp.listener().OnTrackUnpublished(p, mt)\n'
assert source.count(anchor) == 1
source = source.replace(anchor, '\t\tif ti.Source == livekit.TrackSource_SCREEN_SHARE { p.hypercastEndScreen(quotaRoom, quotaParticipant) }\n\t\tif ti.Source == livekit.TrackSource_CAMERA { hypercastRooms.release(quotaRoom, quotaParticipant, ti.Source) }\n' + anchor)
anchor = '\ttrackID := livekit.TrackID(ti.Sid)\n\tmt.AddOnClose('
assert source.count(anchor) == 1
source = source.replace(anchor, '\tquotaRoom, quotaParticipant := p.ClaimGrants().Video.Room, string(p.ID())\n' + anchor)
path.write_text(source)


# LiveKit keeps the MediaEngine instance so transport restarts reuse its state.
# Pion only initializes multi-codec negotiation when copying the MediaEngine.
# Initialize it for both paths so later video sections can introduce codecs.
pion_module = "github.com/pion/webrtc/v4"
module = json.loads(subprocess.check_output(["go", "list", "-m", "-json", pion_module]))
replacement = module["Replace"]
version = f'{replacement["Path"]}@{replacement["Version"]}'
download = json.loads(subprocess.check_output(["go", "mod", "download", "-json", version]))
pion_source = Path("/pion-hypercast")
shutil.copytree(download["Dir"], pion_source)
peer_connection = pion_source / "peerconnection.go"
source = peer_connection.read_text()
old = """		pc.api.mediaEngine = api.mediaEngine.copy()
		pc.api.mediaEngine.setMultiCodecNegotiation(!api.settingEngine.disableMediaEngineMultipleCodecs)
	}
"""
new = """		pc.api.mediaEngine = api.mediaEngine.copy()
	}
	pc.api.mediaEngine.setMultiCodecNegotiation(!api.settingEngine.disableMediaEngineMultipleCodecs)
"""
assert source.count(old) == 1
peer_connection.chmod(0o644)
peer_connection.write_text(source.replace(old, new))
subprocess.run(["go", "mod", "edit", "-replace", f"{pion_module}={pion_source}"], check=True)
