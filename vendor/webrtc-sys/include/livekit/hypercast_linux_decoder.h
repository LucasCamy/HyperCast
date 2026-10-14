#pragma once
#include "api/video_codecs/video_decoder_factory.h"
namespace livekit_ffi {
std::unique_ptr<webrtc::VideoDecoderFactory> CreateHypercastLinuxDecoderFactory();
}
extern "C" unsigned hc_linux_live_decoders();
