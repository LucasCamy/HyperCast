// HyperCast Linux decoder. System FFmpeg is isolated from libwebrtc's
// reduced static FFmpeg. Hardware decode may still require CPU transfer for
// the portable I420 renderer; this path does not claim zero-copy presentation.
#include "livekit/hypercast_linux_decoder.h"
#include "api/make_ref_counted.h"
#include "api/video/i420_buffer.h"
#include "api/video_codecs/video_decoder.h"
#include "rtc_base/time_utils.h"
#include <libyuv.h>
#include <dlfcn.h>
#include <atomic>
#include <vector>
#include <cstring>
#include <stdexcept>
extern "C" {
#include <libavcodec/avcodec.h>
#include <libavutil/hwcontext.h>
#include <libavutil/pixdesc.h>
}
#define HC_STR_(x) #x
#define HC_STR(x) HC_STR_(x)
namespace {
std::atomic<unsigned> live_decoders{0};
struct FF {
  void *codec=nullptr,*util=nullptr;
  template<class T> static T symbol(void* so,const char* name) {
    auto fn=dlsym(so,name);if(!fn)throw std::runtime_error("System FFmpeg symbol missing");
    return reinterpret_cast<T>(fn);
  }
#define FIELD(n) decltype(&n) n##_fn=nullptr
  FIELD(avcodec_find_decoder_by_name);FIELD(avcodec_alloc_context3);FIELD(avcodec_open2);
  FIELD(avcodec_free_context);FIELD(avcodec_send_packet);FIELD(avcodec_receive_frame);
  FIELD(av_packet_alloc);FIELD(av_new_packet);FIELD(av_packet_free);
  FIELD(av_frame_alloc);FIELD(av_frame_free);FIELD(av_frame_unref);
  FIELD(av_hwdevice_ctx_create);FIELD(av_hwframe_transfer_data);FIELD(av_buffer_ref);FIELD(av_buffer_unref);
  FIELD(av_pix_fmt_desc_get);
#undef FIELD
  FF() {
    try {
      const int flags=RTLD_NOW|RTLD_LOCAL|RTLD_DEEPBIND;
      codec=dlopen("libavcodec.so." HC_STR(LIBAVCODEC_VERSION_MAJOR),flags);
      util=dlopen("libavutil.so." HC_STR(LIBAVUTIL_VERSION_MAJOR),flags);
      if(!codec||!util)throw std::runtime_error("System FFmpeg libraries missing");
#define BIND(so,n) n##_fn=symbol<decltype(&n)>(so,#n)
      BIND(codec,avcodec_find_decoder_by_name);BIND(codec,avcodec_alloc_context3);BIND(codec,avcodec_open2);
      BIND(codec,avcodec_free_context);BIND(codec,avcodec_send_packet);BIND(codec,avcodec_receive_frame);
      BIND(codec,av_packet_alloc);BIND(codec,av_new_packet);BIND(codec,av_packet_free);
      BIND(util,av_frame_alloc);BIND(util,av_frame_free);BIND(util,av_frame_unref);
      BIND(util,av_hwdevice_ctx_create);BIND(util,av_hwframe_transfer_data);BIND(util,av_buffer_ref);BIND(util,av_buffer_unref);
      BIND(util,av_pix_fmt_desc_get);
#undef BIND
    }catch(...){close();throw;}
  }
  void close(){if(codec)dlclose(codec);if(util)dlclose(util);codec=util=nullptr;}
  ~FF(){close();}
};
FF& ff(){static FF api;return api;}
}
extern "C" unsigned hc_linux_live_decoders(){return live_decoders.load();}
namespace livekit_ffi {
class LinuxDecoder final:public webrtc::VideoDecoder {
 public:
  explicit LinuxDecoder(const char* name):name_(name){++live_decoders;}
  ~LinuxDecoder(){Release();--live_decoders;}
  bool Configure(const Settings&) override {
    Release();
    try {
      auto& f=ff();
      // Probe GPU devices; if no supported hardware format is offered,
      // get_format selects a software format rather than claiming GPU decode.
      for(auto type:{AV_HWDEVICE_TYPE_CUDA,AV_HWDEVICE_TYPE_VAAPI}) {
        if(f.av_hwdevice_ctx_create_fn(&device_,type,nullptr,nullptr,0)>=0){
          hardware_format_=type==AV_HWDEVICE_TYPE_CUDA?AV_PIX_FMT_CUDA:AV_PIX_FMT_VAAPI;break;
        }
      }
      using_av1_hw_=std::strcmp(name_,"av1")==0 && device_;
      const char* selected=std::strcmp(name_,"av1")==0&&!device_&&f.avcodec_find_decoder_by_name_fn("libdav1d")?"libdav1d":name_;
      auto codec=f.avcodec_find_decoder_by_name_fn(selected);
      if(!codec){Release();return false;}
      context_=f.avcodec_alloc_context3_fn(codec);if(!context_){Release();return false;}
      context_->opaque=this;context_->get_format=&format;
      if(device_)context_->hw_device_ctx=f.av_buffer_ref_fn(device_);
      context_->max_pixels=3840LL*2160;context_->thread_count=2;context_->flags|=AV_CODEC_FLAG_LOW_DELAY;
      context_->pkt_timebase=AVRational{1,90000};
      if(f.avcodec_open2_fn(context_,codec,nullptr)<0){Release();return false;}
      return true;
    }catch(...){Release();return false;}
  }
  int32_t RegisterDecodeCompleteCallback(webrtc::DecodedImageCallback* cb) override {callback_=cb;return 0;}
  int32_t Decode(const webrtc::EncodedImage& input,int64_t) override {
    if(!context_||!callback_||input.size()==0||input.size()>16*1024*1024)return -1;
    auto& f=ff();auto packet=f.av_packet_alloc_fn();if(!packet)return -1;
    if(f.av_new_packet_fn(packet,static_cast<int>(input.size()))<0){f.av_packet_free_fn(&packet);return -1;}
    std::memcpy(packet->data,input.data(),input.size());packet->pts=input.RtpTimestamp();
    int result=f.avcodec_send_packet_fn(context_,packet);f.av_packet_free_fn(&packet);
    if(result<0){
      if(using_av1_hw_ && software_av1())return Decode(input,0);
      return -1;
    }
    auto destroy=[](AVFrame* p){ff().av_frame_free_fn(&p);};
    std::unique_ptr<AVFrame,decltype(destroy)> decoded_owner(f.av_frame_alloc_fn(),destroy),cpu_owner(f.av_frame_alloc_fn(),destroy);
    auto decoded=decoded_owner.get();auto cpu=cpu_owner.get();
    if(!decoded||!cpu)return -1;
    int received=0;
    try {
    while((received=f.avcodec_receive_frame_fn(context_,decoded))==0) {
      AVFrame* source=decoded;
      if(decoded->hw_frames_ctx){
        if(f.av_hwframe_transfer_data_fn(cpu,decoded,0)<0){result=-1;break;}source=cpu;
      }
      if(source->width<=0||source->height<=0||source->width>3840||source->height>2160){result=-1;break;}
      auto buffer=webrtc::I420Buffer::Create(source->width,source->height);
      int converted=-1;
      const int w=source->width,h=source->height,cw=(w+1)/2;
      if(!reported_){fprintf(stderr,"Linux decode: %s, format=%d, %dx%d, hardware=%d\n",name_,source->format,w,h,hardware_used_);reported_=true;}
      if(!source->data[0] || source->linesize[0]<=0 || source->linesize[0]>65536){result=-1;break;}
      if(source->format==AV_PIX_FMT_YUV420P && source->data[1] && source->data[2] &&
          source->linesize[0]>=w && source->linesize[1]>=cw && source->linesize[2]>=cw) {
        converted=libyuv::I420Copy(source->data[0],source->linesize[0],source->data[1],source->linesize[1],source->data[2],source->linesize[2],
          buffer->MutableDataY(),buffer->StrideY(),buffer->MutableDataU(),buffer->StrideU(),buffer->MutableDataV(),buffer->StrideV(),w,h);
      }else if(source->format==AV_PIX_FMT_NV12 && source->data[1] && source->linesize[0]>=w && source->linesize[1]>=cw*2) {
        converted=libyuv::NV12ToI420(source->data[0],source->linesize[0],source->data[1],source->linesize[1],
          buffer->MutableDataY(),buffer->StrideY(),buffer->MutableDataU(),buffer->StrideU(),buffer->MutableDataV(),buffer->StrideV(),w,h);
      }else if(source->format==AV_PIX_FMT_YUV420P10LE && source->data[1] && source->data[2] &&
          source->linesize[0]>=w*2 && source->linesize[1]>=cw*2 && source->linesize[2]>=cw*2) {
        converted=libyuv::I010ToI420(reinterpret_cast<uint16_t*>(source->data[0]),source->linesize[0]/2,
          reinterpret_cast<uint16_t*>(source->data[1]),source->linesize[1]/2,reinterpret_cast<uint16_t*>(source->data[2]),source->linesize[2]/2,
          buffer->MutableDataY(),buffer->StrideY(),buffer->MutableDataU(),buffer->StrideU(),buffer->MutableDataV(),buffer->StrideV(),w,h);
      }else if(source->format==AV_PIX_FMT_P010LE && source->data[1] && source->linesize[0]>=w*2 && source->linesize[1]>=cw*4) {
        std::vector<uint8_t> uv(static_cast<size_t>(cw)*2*((h+1)/2));
        converted=libyuv::P010ToNV12(reinterpret_cast<uint16_t*>(source->data[0]),source->linesize[0]/2,
          reinterpret_cast<uint16_t*>(source->data[1]),source->linesize[1]/2,
          buffer->MutableDataY(),buffer->StrideY(),uv.data(),cw*2,w,h);
        if(converted==0)libyuv::SplitUVPlane(uv.data(),cw*2,buffer->MutableDataU(),buffer->StrideU(),buffer->MutableDataV(),buffer->StrideV(),cw,(h+1)/2);
      }
      if(converted!=0){result=-1;break;}
      auto frame=webrtc::VideoFrame::Builder().set_video_frame_buffer(buffer)
        .set_rtp_timestamp(static_cast<uint32_t>(decoded->pts)).set_timestamp_us(webrtc::TimeMicros()).build();
      callback_->Decoded(frame);f.av_frame_unref_fn(decoded);f.av_frame_unref_fn(cpu);
    }
    }catch(...){return -1;}
    if(received<0 && received!=AVERROR(EAGAIN) && received!=AVERROR_EOF){
      if(using_av1_hw_ && software_av1())return Decode(input,0);
      return -1;
    }
    return result<0?-1:0;
  }
  int32_t Release() override {
    if(context_||device_){auto& f=ff();if(context_)f.avcodec_free_context_fn(&context_);
      if(device_)f.av_buffer_unref_fn(&device_);}
    return 0;
  }
  DecoderInfo GetDecoderInfo() const override {DecoderInfo info;info.implementation_name="HyperCast Linux FFmpeg";
    info.is_hardware_accelerated=hardware_used_;return info;}
 private:
  static AVPixelFormat format(AVCodecContext* ctx,const AVPixelFormat* formats){
    auto self=static_cast<LinuxDecoder*>(ctx->opaque);
    if(self->device_)for(auto p=formats;*p!=AV_PIX_FMT_NONE;++p)if(*p==self->hardware_format_){self->hardware_used_=true;return *p;}
    self->hardware_used_=false;
    for(auto p=formats;*p!=AV_PIX_FMT_NONE;++p){auto desc=ff().av_pix_fmt_desc_get_fn(*p);if(desc && !(desc->flags&AV_PIX_FMT_FLAG_HWACCEL))return *p;}
    return AV_PIX_FMT_NONE;
  }
  bool software_av1(){
    Release();using_av1_hw_=false;hardware_used_=false;
    auto& f=ff();auto codec=f.avcodec_find_decoder_by_name_fn("libdav1d");if(!codec)return false;
    context_=f.avcodec_alloc_context3_fn(codec);if(!context_)return false;
    context_->max_pixels=3840LL*2160;context_->thread_count=2;context_->flags|=AV_CODEC_FLAG_LOW_DELAY;context_->pkt_timebase=AVRational{1,90000};
    if(f.avcodec_open2_fn(context_,codec,nullptr)<0){Release();return false;}return true;
  }
  bool using_av1_hw_=false;
  const char* name_;AVCodecContext* context_=nullptr;AVBufferRef* device_=nullptr;
  bool reported_=false;AVPixelFormat hardware_format_=AV_PIX_FMT_NONE;bool hardware_used_=false;
  webrtc::DecodedImageCallback* callback_=nullptr; // Non-owning; WebRTC owns decoder and callback.
};
class LinuxFactory final:public webrtc::VideoDecoderFactory {
 public:
  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override {
    try{auto& f=ff();std::vector<webrtc::SdpVideoFormat> formats;
      if(f.avcodec_find_decoder_by_name_fn("hevc"))formats.emplace_back("H265",webrtc::SdpVideoFormat::Parameters{{"profile-id","1"},{"tier-flag","0"},{"level-id","156"}});
      if(f.avcodec_find_decoder_by_name_fn("libdav1d")||f.avcodec_find_decoder_by_name_fn("av1"))formats.emplace_back("AV1",webrtc::SdpVideoFormat::Parameters{{"profile","0"},{"level-idx","14"},{"tier","0"}});
      return formats;
    }catch(...){return {};}
  }
  std::unique_ptr<webrtc::VideoDecoder> Create(const webrtc::Environment&,const webrtc::SdpVideoFormat& format) override {
    if(format.name=="H265")return std::make_unique<LinuxDecoder>("hevc");
    if(format.name=="AV1")return std::make_unique<LinuxDecoder>(ff().avcodec_find_decoder_by_name_fn("av1")?"av1":"libdav1d");
    return nullptr;
  }
};
class TestCallback final:public webrtc::DecodedImageCallback {
 public:
  int32_t Decoded(webrtc::VideoFrame& frame) override {++frames;width=frame.width();height=frame.height();return 0;}
  unsigned frames=0;int width=0,height=0;
};
extern "C" int hc_linux_decode_test(const uint8_t* data,size_t length,int width,int height){
  if(!data||length==0||length>16*1024*1024)return -1;
  unsigned before=live_decoders.load();TestCallback callback;
  {
    LinuxDecoder decoder("hevc");webrtc::VideoDecoder::Settings settings;
    settings.set_codec_type(webrtc::kVideoCodecH265);
    if(!decoder.Configure(settings))return -2;
    decoder.RegisterDecodeCompleteCallback(&callback);
    webrtc::EncodedImage image;image.SetEncodedData(webrtc::EncodedImageBuffer::Create(data,length));
    image.SetRtpTimestamp(90000);
    if(decoder.Decode(image,0)<0)return -3;
    decoder.Release();
  }
  if(live_decoders.load()!=before)return -4;
  return callback.frames>0 && callback.width==width && callback.height==height?0:-5;
}
std::unique_ptr<webrtc::VideoDecoderFactory> CreateHypercastLinuxDecoderFactory(){return std::make_unique<LinuxFactory>();}
}
