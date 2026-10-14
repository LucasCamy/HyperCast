// HyperCast extension. LGPL FFmpeg is dynamically linked; decoded pixels never
// leave D3D11 video surfaces. No software decoder fallback for AV1/HEVC.
#define NOMINMAX
#include <windows.h>
#include <d3d11.h>
#include <wrl/client.h>
#include <mutex>
#include <unordered_map>
#include <cstdio>
#include <string>
#include <stdexcept>
#include "livekit/hypercast_d3d11_decoder.h"
#include "api/make_ref_counted.h"
#include "api/video_codecs/video_decoder.h"
#include "rtc_base/time_utils.h"
extern "C" {
#include <libavcodec/avcodec.h>
#include <libavutil/hwcontext.h>
#include <libavutil/hwcontext_d3d11va.h>
}
namespace livekit_ffi {
using Microsoft::WRL::ComPtr;
// libwebrtc embeds a reduced FFmpeg with identically named C symbols. Resolve
// the pinned shared DLL explicitly so no allocation/layout crosses that ABI.
struct FFAPI {
  HMODULE codec = nullptr, util = nullptr;
  template<class T> static T symbol(HMODULE dll, const char* name) {
    auto address = GetProcAddress(dll, name);
    if (!address) throw std::runtime_error(std::string("Missing FFmpeg symbol ") + name);
    return reinterpret_cast<T>(address);
  }
#define FF_FIELD(name) decltype(&name) name##_ptr = nullptr
  FF_FIELD(avcodec_find_decoder_by_name); FF_FIELD(avcodec_alloc_context3);
  FF_FIELD(avcodec_open2); FF_FIELD(avcodec_free_context);
  FF_FIELD(avcodec_send_packet); FF_FIELD(avcodec_receive_frame);
  FF_FIELD(av_packet_alloc); FF_FIELD(av_new_packet); FF_FIELD(av_packet_free);
  FF_FIELD(av_hwdevice_ctx_create); FF_FIELD(av_buffer_ref); FF_FIELD(av_buffer_unref);
  FF_FIELD(av_frame_alloc); FF_FIELD(av_frame_clone); FF_FIELD(av_frame_free); FF_FIELD(av_frame_unref);
#undef FF_FIELD
  FFAPI() {
    wchar_t exe[32768]; DWORD length = GetModuleFileNameW(nullptr, exe, 32768);
    if (!length || length >= 32768) throw std::runtime_error("Cannot resolve application directory");
    std::wstring root(exe, length); root = root.substr(0, root.find_last_of(L"\\/"));
    codec = LoadLibraryExW((root+L"\\avcodec-63.dll").c_str(), nullptr, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR|LOAD_LIBRARY_SEARCH_SYSTEM32);
    util = LoadLibraryExW((root+L"\\avutil-61.dll").c_str(), nullptr, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR|LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (!codec || !util) throw std::runtime_error("Pinned FFmpeg DLLs missing beside HyperCast");
#define FF_BIND(dll, name) name##_ptr = symbol<decltype(&name)>(dll, #name)
    FF_BIND(codec,avcodec_find_decoder_by_name); FF_BIND(codec,avcodec_alloc_context3);
    FF_BIND(codec,avcodec_open2); FF_BIND(codec,avcodec_free_context);
    FF_BIND(codec,avcodec_send_packet); FF_BIND(codec,avcodec_receive_frame);
    FF_BIND(codec,av_packet_alloc); FF_BIND(codec,av_new_packet); FF_BIND(codec,av_packet_free);
    FF_BIND(util,av_hwdevice_ctx_create); FF_BIND(util,av_buffer_ref); FF_BIND(util,av_buffer_unref);
    FF_BIND(util,av_frame_alloc); FF_BIND(util,av_frame_clone); FF_BIND(util,av_frame_free); FF_BIND(util,av_frame_unref);
#undef FF_BIND
  }
};
static FFAPI& ff() { static FFAPI api; return api; }
#define avcodec_find_decoder_by_name ff().avcodec_find_decoder_by_name_ptr
#define avcodec_alloc_context3 ff().avcodec_alloc_context3_ptr
#define avcodec_open2 ff().avcodec_open2_ptr
#define avcodec_free_context ff().avcodec_free_context_ptr
#define avcodec_send_packet ff().avcodec_send_packet_ptr
#define avcodec_receive_frame ff().avcodec_receive_frame_ptr
#define av_packet_alloc ff().av_packet_alloc_ptr
#define av_new_packet ff().av_new_packet_ptr
#define av_packet_free ff().av_packet_free_ptr
#define av_hwdevice_ctx_create ff().av_hwdevice_ctx_create_ptr
#define av_buffer_ref ff().av_buffer_ref_ptr
#define av_buffer_unref ff().av_buffer_unref_ptr
#define av_frame_alloc ff().av_frame_alloc_ptr
#define av_frame_clone ff().av_frame_clone_ptr
#define av_frame_free ff().av_frame_free_ptr
#define av_frame_unref ff().av_frame_unref_ptr
struct Surface { uintptr_t texture; uint32_t slice; };
static std::mutex registry_mutex;
static std::unordered_map<const webrtc::VideoFrameBuffer*, Surface> registry;
class GPUBuffer : public webrtc::VideoFrameBuffer {
 public:
  explicit GPUBuffer(AVFrame* decoded) : frame_(av_frame_clone(decoded)) {
    if (!frame_) throw std::bad_alloc();
    try {
    std::lock_guard<std::mutex> guard(registry_mutex);
    registry[this] = {reinterpret_cast<uintptr_t>(frame_->data[0]), static_cast<uint32_t>(reinterpret_cast<uintptr_t>(frame_->data[1]))};
    } catch (...) { av_frame_free(&frame_); throw; }
  }
  ~GPUBuffer() override {
    { std::lock_guard<std::mutex> guard(registry_mutex); registry.erase(this); }
    av_frame_free(&frame_);
  }
  Type type() const override { return Type::kNative; }
  int width() const override { return frame_->width; }
  int height() const override { return frame_->height; }
  webrtc::scoped_refptr<webrtc::I420BufferInterface> ToI420() override { return nullptr; }
 private: AVFrame* frame_;
};
uintptr_t HypercastD3D11Texture(const webrtc::VideoFrameBuffer* buffer) {
  std::lock_guard<std::mutex> guard(registry_mutex);
  auto found = registry.find(buffer); return found == registry.end() ? 0 : found->second.texture;
}
uint32_t HypercastD3D11Slice(const webrtc::VideoFrameBuffer* buffer) {
  std::lock_guard<std::mutex> guard(registry_mutex);
  auto found = registry.find(buffer); return found == registry.end() ? 0 : found->second.slice;
}
uint32_t HypercastD3D11LiveSurfaces() {
  std::lock_guard<std::mutex> guard(registry_mutex);
  return static_cast<uint32_t>(registry.size());
}
class GPUDecoder : public webrtc::VideoDecoder {
 public:
  explicit GPUDecoder(AVCodecID codec) : codec_(codec) {}
  ~GPUDecoder() override { Release(); }
  bool Configure(const Settings&) override {
    try {
    Release();
    const AVCodec* decoder = avcodec_find_decoder_by_name(codec_ == AV_CODEC_ID_AV1 ? "av1" : "hevc");
    if (!decoder) { fprintf(stderr, "GPU decoder: codec unavailable\n"); return false; }
    context_ = avcodec_alloc_context3(decoder);
    if (!context_) return false;
    // FFmpeg creates a hardware D3D11 device and protects its immediate context.
    int hardware = av_hwdevice_ctx_create(&device_, AV_HWDEVICE_TYPE_D3D11VA, nullptr, nullptr, 0);
    if (hardware < 0) { fprintf(stderr, "GPU decoder: D3D11 device error %d\n", hardware); return false; }
    context_->hw_device_ctx = av_buffer_ref(device_);
    if (!context_->hw_device_ctx) return false;
    context_->get_format = [](AVCodecContext*, const AVPixelFormat* formats) {
      for (auto p = formats; *p != AV_PIX_FMT_NONE; ++p) if (*p == AV_PIX_FMT_D3D11) return *p;
      return AV_PIX_FMT_NONE;
    };
    context_->thread_count = 1;
    context_->max_pixels = 33554432LL; // Bound allocation before receiving a surface.
    context_->flags |= AV_CODEC_FLAG_LOW_DELAY;
    context_->extra_hw_frames = 4;
    int opened = avcodec_open2(context_, decoder, nullptr);
    fprintf(stderr, "GPU decoder configure: %s status %d\n", decoder->name, opened);
    return opened >= 0;
    } catch (const std::exception& error) { fprintf(stderr, "GPU decoder: %s\n", error.what()); return false; }
  }
  int32_t RegisterDecodeCompleteCallback(webrtc::DecodedImageCallback* callback) override { callback_ = callback; return 0; }
  int32_t Decode(const webrtc::EncodedImage& encoded, int64_t) override {
    if (!context_ || !callback_ || encoded.size() == 0 || encoded.size() > 16 * 1024 * 1024) return -1;
    AVPacket* packet = av_packet_alloc();
    if (!packet) return -1;
    if (av_new_packet(packet, static_cast<int>(encoded.size())) < 0) { av_packet_free(&packet); return -1; }
    memcpy(packet->data, encoded.data(), encoded.size());
    packet->pts = encoded.RtpTimestamp();
    int status = avcodec_send_packet(context_, packet);
    av_packet_free(&packet);
    if (status < 0 && status != AVERROR(EAGAIN)) { fprintf(stderr, "GPU decode packet error %d\n", status); return -1; }
    AVFrame* decoded = av_frame_alloc();
    if (!decoded) return -1;
    while (avcodec_receive_frame(context_, decoded) == 0) {
      if (decoded->format != AV_PIX_FMT_D3D11 || !decoded->data[0] || decoded->width <= 0 || decoded->height <= 0 || decoded->width > 8192 || decoded->height > 8192 || int64_t(decoded->width)*decoded->height > 33554432LL) {
        av_frame_free(&decoded); return -1;
      }
      try {
      auto buffer = webrtc::make_ref_counted<GPUBuffer>(decoded);
      auto frame = webrtc::VideoFrame::Builder().set_video_frame_buffer(buffer)
          .set_rtp_timestamp(static_cast<uint32_t>(decoded->pts)).set_timestamp_us(webrtc::TimeMicros()).build();
      callback_->Decoded(frame);
      } catch (...) { av_frame_free(&decoded); return -1; }
      av_frame_unref(decoded);
    }
    av_frame_free(&decoded);
    return 0;
  }
  int32_t Release() override { if (context_) avcodec_free_context(&context_); if (device_) av_buffer_unref(&device_); return 0; }
  DecoderInfo GetDecoderInfo() const override { DecoderInfo info; info.implementation_name = "HyperCast D3D11VA GPU"; info.is_hardware_accelerated = true; return info; }
 private:
  AVCodecID codec_;
  AVCodecContext* context_ = nullptr;
  AVBufferRef* device_ = nullptr;
  webrtc::DecodedImageCallback* callback_ = nullptr;
};
class GPUFactory : public webrtc::VideoDecoderFactory {
 public:
  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override {
    return {webrtc::SdpVideoFormat("AV1", {{"profile", "0"}, {"level-idx", "14"}, {"tier", "0"}}),
            webrtc::SdpVideoFormat("H265", {{"profile-id", "1"}, {"tier-flag", "0"}, {"level-id", "156"}})};
  }
  std::unique_ptr<webrtc::VideoDecoder> Create(const webrtc::Environment&, const webrtc::SdpVideoFormat& format) override {
    if (format.name == "AV1") return std::make_unique<GPUDecoder>(AV_CODEC_ID_AV1);
    if (format.name == "H265") return std::make_unique<GPUDecoder>(AV_CODEC_ID_HEVC);
    return nullptr;
  }
};
std::unique_ptr<webrtc::VideoDecoderFactory> CreateHypercastD3D11DecoderFactory() { return std::make_unique<GPUFactory>(); }
}
