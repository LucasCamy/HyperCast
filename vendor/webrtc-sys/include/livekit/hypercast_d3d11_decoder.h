#pragma once
#include "api/video_codecs/video_decoder_factory.h"
#include "api/video/video_frame_buffer.h"
namespace livekit_ffi {
std::unique_ptr<webrtc::VideoDecoderFactory> CreateHypercastD3D11DecoderFactory();
uintptr_t HypercastD3D11Texture(const webrtc::VideoFrameBuffer* buffer);
uint32_t HypercastD3D11Slice(const webrtc::VideoFrameBuffer* buffer);
uint32_t HypercastD3D11LiveSurfaces();
}
