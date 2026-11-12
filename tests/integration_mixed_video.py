"""Exercise the room's six video subscriptions with generated frames on a local SFU."""
import asyncio
import os
import time
import uuid

from livekit import rtc
from livekit.rtc.participant import PublishTrackError
from tools import room_token


async def main():
    key, secret = room_token.load_env()
    room_name = "mixed-video-" + uuid.uuid4().hex[:12]
    url = os.environ.get("HYPERCAST_TEST_URL", "ws://127.0.0.1:8100")
    rooms = []
    streams = []
    readers = []
    sources = {}
    received = set()
    screen = rtc.TrackSource.SOURCE_SCREENSHARE
    camera = rtc.TrackSource.SOURCE_CAMERA

    async def connect(viewer=False, setup=None):
        now = int(time.time())
        token = room_token.sign({
            "iss": key, "sub": "hc-member-" + uuid.uuid4().hex,
            "nbf": now - 5, "exp": now + 120,
            "video": {"room": room_name, "roomJoin": True, "canSubscribe": viewer,
                      "canPublish": not viewer, "canPublishSources": ["screen_share", "camera"]},
        }, secret)
        room = rtc.Room()
        rooms.append(room)
        if setup:
            setup(room)
        await room.connect(url, token)
        return room

    async def publish(room, kind):
        source = rtc.VideoSource(320, 180, is_screencast=kind == screen)
        track = rtc.LocalVideoTrack.create_video_track("screen" if kind == screen else "camera", source)
        result = await room.local_participant.publish_track(track, rtc.TrackPublishOptions(
            source=kind, video_codec=rtc.VideoCodec.H264, simulcast=False))
        sources[result.sid] = source
        return result

    async def feed():
        data = bytes([16]) * (320 * 180) + bytes([128]) * (320 * 180 // 2)
        frame = rtc.VideoFrame(320, 180, rtc.VideoBufferType.I420, data)
        while True:
            for source in tuple(sources.values()):
                source.capture_frame(frame)
            await asyncio.sleep(1 / 15)

    async def watch(track, publication):
        stream = rtc.VideoStream(track, capacity=1)
        streams.append(stream)
        async for _ in stream:
            received.add(publication.sid)
            return

    def setup(room):
        @room.on("track_subscribed")
        def subscribed(track, publication, _participant):
            readers.append(asyncio.create_task(watch(track, publication)))

    async def wait_for(sids):
        async with asyncio.timeout(20):
            while not sids.issubset(received):
                await asyncio.sleep(0.05)

    publisher_task = asyncio.create_task(feed())
    try:
        await connect(viewer=True, setup=setup)
        screen_publishers = [await connect() for _ in range(3)]
        camera_publishers = [await connect() for _ in range(3)]
        spare = await connect()
        screens = [await publish(room, screen) for room in screen_publishers]
        cameras = [await publish(room, camera) for room in camera_publishers]
        await wait_for({publication.sid for publication in screens + cameras})
        for source in (screen, camera):
            try:
                await publish(spare, source)
            except PublishTrackError:
                pass
            else:
                raise AssertionError("Seventh video was accepted")
        await screen_publishers[0].disconnect()
        sources.pop(screens[0].sid)
        replacement = await publish(spare, camera)
        await wait_for({replacement.sid})
        print("PASS: received three screens and three cameras, rejected seventh video, received replacement camera")
    finally:
        publisher_task.cancel()
        for reader in readers:
            reader.cancel()
        await asyncio.gather(publisher_task, *readers, return_exceptions=True)
        await asyncio.gather(*(stream.aclose() for stream in streams))
        await asyncio.gather(*(room.disconnect() for room in rooms))


if __name__ == "__main__":
    asyncio.run(main())
