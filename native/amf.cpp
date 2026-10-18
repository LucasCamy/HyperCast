// AMD hardware encoding: DXGI/WGC -> private BGRA -> GPU NV12 -> AMF.
// Only the compressed bitstream crosses into CPU memory. One frame in flight.
#define NOMINMAX
#include <windows.h>
#include <d3d11.h>
#include <d3d10.h>
#include <dxgi.h>
#include <wrl/client.h>
#include <amf/core/Factory.h>
#include <amf/core/Buffer.h>
#include <amf/components/VideoConverter.h>
#include <amf/components/VideoEncoderHEVC.h>
#include <amf/components/VideoEncoderAV1.h>
#include <algorithm>
#include <cstring>
#include <limits>
#include <memory>
#include <atomic>
static std::atomic<unsigned> live_encoders{0};
#include <stdexcept>
#include <string>
#include "capture_color.h"
#include "capture_size.h"
#include <amf/components/ComponentCaps.h>
using Microsoft::WRL::ComPtr;
namespace {
void report(char* error,size_t capacity,const char* message) noexcept {
    if(error && capacity){size_t n=std::min(capacity-1,std::strlen(message));std::memcpy(error,message,n);error[n]=0;}
}
void check(AMF_RESULT result,const char* operation) {
    if(result!=AMF_OK)throw std::runtime_error(std::string(operation)+": AMF status "+std::to_string(result));
}
void hr(HRESULT result,const char* operation){if(FAILED(result))throw std::runtime_error(std::string(operation)+": HRESULT "+std::to_string(result));}
struct Encoder {
    Encoder(){++live_encoders;}
    HMODULE dll=nullptr;
    amf::AMFContextPtr amf_context;
    amf::AMFComponentPtr converter,encoder;
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> context;
    ComPtr<ID3D11Texture2D> input;
    ComPtr<ID3D11Query> fence;
    CaptureColor color;
    uint32_t width=0,height=0,fps=0,source_width=0,source_height=0;
    bool av1=false,ready=false,initialized=false;
    uint32_t profile=0,applied_bitrate=0,vbv_bits=0,vbv_frames=2,submissions=0;
    int last_qp=-1;bool vbv_runtime=true;

    ~Encoder() noexcept {
        --live_encoders;
        if(encoder){encoder->Terminate();encoder=nullptr;}
        if(converter){converter->Terminate();converter=nullptr;}
        if(amf_context){amf_context->Terminate();amf_context=nullptr;}
        if(dll)FreeLibrary(dll);
    }
    template<class T> void property(const wchar_t* name,T value,const char* operation){check(encoder->SetProperty(name,value),operation);}
    void open(ID3D11Device* d,uint32_t w,uint32_t h,uint32_t f,uint32_t bitrate,bool use_av1,uint32_t selected_profile=0,uint32_t limit_w=8192,uint32_t limit_h=8192){
        if(!d || !hc_capture_size(w,h) || f<1 || f>240 || bitrate<1000000 || bitrate>45000000)
            throw std::runtime_error("Invalid AMF configuration (maximum 8192 per axis and 32 megapixels, 1-240 FPS, 1-45 Mbps)");
        if(selected_profile>2)throw std::runtime_error("Invalid AMF profile");
        profile=selected_profile;vbv_frames=profile==2?1:2;
        source_width=w;source_height=h;fps=f;av1=use_av1;device=d;
        ComPtr<IDXGIDevice> dxgi;ComPtr<IDXGIAdapter> adapter;DXGI_ADAPTER_DESC desc{};
        hr(device.As(&dxgi),"Query AMF adapter");hr(dxgi->GetAdapter(&adapter),"Get AMF adapter");hr(adapter->GetDesc(&desc),"Describe AMF adapter");
        if(desc.VendorId!=0x1002)throw std::runtime_error("AMF requires a capture device on an AMD GPU");
        device->GetImmediateContext(&context);
        ComPtr<ID3D10Multithread> protect;hr(context.As(&protect),"Protect AMF D3D11 context");protect->SetMultithreadProtected(TRUE);
        dll=LoadLibraryExW(L"amfrt64.dll",nullptr,LOAD_LIBRARY_SEARCH_SYSTEM32);
        if(!dll)throw std::runtime_error("AMD AMF não encontrado. Reinstale o driver AMD Adrenalin e reinicie o Windows.");
        auto init=reinterpret_cast<AMFInit_Fn>(GetProcAddress(dll,AMF_INIT_FUNCTION_NAME));
        auto version=reinterpret_cast<AMFQueryVersion_Fn>(GetProcAddress(dll,AMF_QUERY_VERSION_FUNCTION_NAME));
        if(!init || !version)throw std::runtime_error("Driver AMD sem API AMF válida; reinstale o Adrenalin.");
        amf_uint64 runtime=0;check(version(&runtime),"Query AMF runtime version");
        if(AMF_GET_MAJOR_VERSION(runtime)!=1 || runtime<AMF_MAKE_FULL_VERSION(1,4,18,0))throw std::runtime_error("Driver AMD AMF antigo; atualize o Adrenalin.");
        amf::AMFFactory* factory=nullptr;check(init(std::min(runtime,static_cast<amf_uint64>(AMF_FULL_VERSION)),&factory),"Initialize AMF runtime");
        if(!factory)throw std::runtime_error("Invalid AMF factory");
        check(factory->CreateContext(&amf_context),"Create AMF context");
        check(amf_context->InitDX11(device.Get()),"Initialize AMF D3D11 interop");
        AMF_RESULT result=factory->CreateComponent(amf_context,av1?AMFVideoEncoder_AV1:AMFVideoEncoder_HEVC,&encoder);
        if(result!=AMF_OK || !encoder)throw std::runtime_error(av1?"Esta GPU AMD não oferece codificação AV1 por hardware. Selecione HEVC (H.265).":"HEVC por hardware indisponível nesta GPU/driver AMD.");
        amf::AMFCapsPtr caps;check(encoder->GetCaps(&caps),"Query AMF encoder capabilities");
        if(!caps)throw std::runtime_error("Invalid AMF encoder capabilities");
        amf::AMFIOCapsPtr input_caps;check(caps->GetInputCaps(&input_caps),"Query AMF input capabilities");
        if(!input_caps)throw std::runtime_error("Invalid AMF input capabilities");
        amf_int32 min_w=0,max_w=0,min_h=0,max_h=0;
        input_caps->GetWidthRange(&min_w,&max_w);input_caps->GetHeightRange(&min_h,&max_h);
        if(max_w<2 || max_h<2)throw std::runtime_error("Invalid AMF resolution capabilities");
        hc_fit_size(w,h,std::min(uint32_t(max_w),limit_w),std::min(uint32_t(max_h),limit_h));width=w;height=h;
        if(w<uint32_t(std::max(2,min_w)) || h<uint32_t(std::max(2,min_h)))throw std::runtime_error("Source smaller than AMF minimum resolution");
        if(av1){
            property(AMF_VIDEO_ENCODER_AV1_USAGE,amf_int64(AMF_VIDEO_ENCODER_AV1_USAGE_ULTRA_LOW_LATENCY),"Set AMF AV1 low latency");
            property(AMF_VIDEO_ENCODER_AV1_QUALITY_PRESET,amf_int64(AMF_VIDEO_ENCODER_AV1_QUALITY_PRESET_SPEED),"Set AMF AV1 speed");
            property(AMF_VIDEO_ENCODER_AV1_RATE_CONTROL_METHOD,amf_int64(AMF_VIDEO_ENCODER_AV1_RATE_CONTROL_METHOD_CBR),"Set AMF AV1 CBR");
            property(AMF_VIDEO_ENCODER_AV1_FRAMESIZE,AMFConstructSize(w,h),"Set AMF AV1 frame size");
            property(AMF_VIDEO_ENCODER_AV1_GOP_SIZE,amf_int64(f),"Set AMF AV1 GOP");
            property(AMF_VIDEO_ENCODER_AV1_HEADER_INSERTION_MODE,amf_int64(AMF_VIDEO_ENCODER_AV1_HEADER_INSERTION_MODE_KEY_FRAME_ALIGNED),"Set AMF AV1 headers");
            property(AMF_VIDEO_ENCODER_AV1_PRE_ANALYSIS_ENABLE,false,"Disable AMF AV1 lookahead");
        }else{
            property(AMF_VIDEO_ENCODER_HEVC_USAGE,amf_int64(AMF_VIDEO_ENCODER_HEVC_USAGE_ULTRA_LOW_LATENCY),"Set AMF HEVC low latency");
            property(AMF_VIDEO_ENCODER_HEVC_QUALITY_PRESET,amf_int64(profile==1?AMF_VIDEO_ENCODER_HEVC_QUALITY_PRESET_BALANCED:AMF_VIDEO_ENCODER_HEVC_QUALITY_PRESET_SPEED),"Set AMF HEVC speed");
            property(AMF_VIDEO_ENCODER_HEVC_RATE_CONTROL_METHOD,amf_int64(AMF_VIDEO_ENCODER_HEVC_RATE_CONTROL_METHOD_CBR),"Set AMF HEVC CBR");
            property(AMF_VIDEO_ENCODER_HEVC_FRAMESIZE,AMFConstructSize(w,h),"Set AMF HEVC frame size");
            property(AMF_VIDEO_ENCODER_HEVC_GOP_SIZE,amf_int64(profile==1?f*2:f),"Set AMF HEVC GOP");
            property(AMF_VIDEO_ENCODER_HEVC_HEADER_INSERTION_MODE,amf_int64(AMF_VIDEO_ENCODER_HEVC_HEADER_INSERTION_MODE_IDR_ALIGNED),"Set AMF HEVC headers");
            property(AMF_VIDEO_ENCODER_HEVC_PRE_ANALYSIS_ENABLE,false,"Disable AMF HEVC lookahead");
        }
        rates(bitrate,f);
        property(av1?AMF_VIDEO_ENCODER_AV1_RATE_CONTROL_SKIP_FRAME:AMF_VIDEO_ENCODER_HEVC_RATE_CONTROL_SKIP_FRAME_ENABLE,
                 false,"Disable AMF rate-control frame skips");
        result=encoder->Init(amf::AMF_SURFACE_NV12,w,h);
        if(result!=AMF_OK)throw std::runtime_error(av1?"AV1 por hardware indisponível nesta GPU AMD. Selecione HEVC (H.265).":"Não foi possível iniciar HEVC na AMD (AMF status "+std::to_string(result)+"). Confira o driver e o diagnóstico.");
        initialized=true;
        check(factory->CreateComponent(amf_context,AMFVideoConverter,&converter),"Create GPU AMF converter");
        check(converter->SetProperty(AMF_VIDEO_CONVERTER_COMPUTE_DEVICE,amf_int64(amf::AMF_MEMORY_DX11)),"Set AMF DX11 compute");
        check(converter->SetProperty(AMF_VIDEO_CONVERTER_MEMORY_TYPE,amf_int64(amf::AMF_MEMORY_DX11)),"Set AMF GPU converter memory");
        check(converter->SetProperty(AMF_VIDEO_CONVERTER_OUTPUT_FORMAT,amf_int64(amf::AMF_SURFACE_NV12)),"Set AMF NV12 output");
        check(converter->SetProperty(AMF_VIDEO_CONVERTER_COLOR_PROFILE,amf_int64(AMF_VIDEO_CONVERTER_COLOR_PROFILE_709)),"Set AMF Rec709 conversion");
        check(converter->Init(amf::AMF_SURFACE_BGRA,w,h),"Initialize GPU AMF color conversion");
        D3D11_TEXTURE2D_DESC texture{};texture.Width=w;texture.Height=h;texture.MipLevels=1;texture.ArraySize=1;
        texture.Format=DXGI_FORMAT_B8G8R8A8_UNORM;texture.SampleDesc.Count=1;texture.BindFlags=D3D11_BIND_SHADER_RESOURCE|D3D11_BIND_RENDER_TARGET;
        hr(device->CreateTexture2D(&texture,nullptr,&input),"Allocate AMF GPU capture copy");
        D3D11_QUERY_DESC query{D3D11_QUERY_EVENT,0};hr(device->CreateQuery(&query,&fence),"Create AMF GPU fence");
    }
    void rates(uint32_t bitrate,uint32_t f){
        if(bitrate<64000 || bitrate>45000000 || f<1 || f>240)throw std::runtime_error("Invalid AMF bitrate/framerate");
        if(applied_bitrate==bitrate && fps==f)return;
        const auto target=av1?AMF_VIDEO_ENCODER_AV1_TARGET_BITRATE:AMF_VIDEO_ENCODER_HEVC_TARGET_BITRATE;
        property(target,amf_int64(bitrate),"Set AMF bitrate");
        if(!initialized || fps!=f)property(av1?AMF_VIDEO_ENCODER_AV1_FRAMERATE:AMF_VIDEO_ENCODER_HEVC_FRAMERATE,AMFConstructRate(f,1),"Set AMF framerate");
        const auto vbv=av1?AMF_VIDEO_ENCODER_AV1_VBV_BUFFER_SIZE:AMF_VIDEO_ENCODER_HEVC_VBV_BUFFER_SIZE;
        const amf_int64 budget=std::min<amf_int64>(45000000,std::max<amf_int64>(16000,(amf_int64(bitrate)*vbv_frames)/f));
        AMF_RESULT buffer_result=encoder->SetProperty(vbv,budget);
        if(buffer_result!=AMF_OK && !initialized)check(buffer_result,"Set AMF VBV");
        vbv_runtime=buffer_result==AMF_OK;
        amf_int64 actual_rate=0,actual_buffer=0;
        check(encoder->GetProperty(target,&actual_rate),"Read AMF bitrate");
        check(encoder->GetProperty(vbv,&actual_buffer),"Read AMF VBV");
        if(actual_rate<64000 || actual_rate>bitrate || actual_buffer<0 || actual_buffer>45000000)
            throw std::runtime_error("AMF rate exceeds requested network budget");
        applied_bitrate=uint32_t(actual_rate);vbv_bits=uint32_t(actual_buffer);fps=f;
    }
    void wait_copy(){
        context->End(fence.Get());context->Flush();ULONGLONG start=GetTickCount64();
        for(;;){HRESULT result=context->GetData(fence.Get(),nullptr,0,D3D11_ASYNC_GETDATA_DONOTFLUSH);hr(result,"Wait AMF GPU copy");
            if(result==S_OK)return;if(GetTickCount64()-start>1000)throw std::runtime_error("AMF GPU copy timed out");Sleep(1);}
    }
    static amf::AMFDataPtr output(amf::AMFComponent* component){
        ULONGLONG start=GetTickCount64();amf::AMFDataPtr data;
        for(;;){AMF_RESULT result=component->QueryOutput(&data);
            if(result==AMF_OK && data)return data;
            if(result!=AMF_REPEAT && result!=AMF_NEED_MORE_INPUT && result!=AMF_OK)check(result,"Query AMF output");
            if(GetTickCount64()-start>1000)throw std::runtime_error("AMF output timed out; encoder needs to be recreated");Sleep(1);}
    }
    void encode(ID3D11Texture2D* source,uint64_t timestamp,bool key,uint8_t* bytes,size_t capacity,size_t& length,bool& is_key){
        if(!bytes || (!source && !ready) || timestamp>static_cast<uint64_t>(std::numeric_limits<amf_pts>::max()/10))throw std::runtime_error("Invalid AMF frame/output");
        if(source){
            D3D11_TEXTURE2D_DESC desc{};source->GetDesc(&desc);ComPtr<ID3D11Device> owner;source->GetDevice(&owner);
            if(owner.Get()!=device.Get())throw std::runtime_error("Capture device changed; rebuild GPU pipeline");
            if(desc.Width!=source_width || desc.Height!=source_height)throw std::runtime_error("Capture dimensions changed; rebuild GPU pipeline");
            if(desc.SampleDesc.Count!=1 || desc.ArraySize!=1 || desc.MipLevels!=1 || (desc.Format!=DXGI_FORMAT_B8G8R8A8_UNORM && desc.Format!=DXGI_FORMAT_R16G16B16A16_FLOAT))throw std::runtime_error("Unsupported capture texture layout for AMD AMF");
            if(desc.Format==DXGI_FORMAT_R16G16B16A16_FLOAT || desc.Width!=width || desc.Height!=height)color.convert(device.Get(),context.Get(),source,input.Get());
            else context->CopyResource(input.Get(),source);
            wait_copy();ready=true;
        }
        amf::AMFSurfacePtr surface;check(amf_context->CreateSurfaceFromDX11Native(input.Get(),&surface,nullptr),"Wrap AMF DX11 texture");
        surface->SetPts(static_cast<amf_pts>(timestamp)*10);surface->SetDuration(10000000/fps);
        check(converter->SubmitInput(surface),"Submit AMF GPU conversion");
        auto converted=output(converter);amf::AMFSurfacePtr nv12(converted);
        if(!nv12 || nv12->GetMemoryType()!=amf::AMF_MEMORY_DX11 || nv12->GetFormat()!=amf::AMF_SURFACE_NV12)throw std::runtime_error("AMF converter did not return a GPU NV12 surface");
        nv12->SetPts(surface->GetPts());nv12->SetDuration(surface->GetDuration());
        if(key){
            check(nv12->SetProperty(av1?AMF_VIDEO_ENCODER_AV1_FORCE_FRAME_TYPE:AMF_VIDEO_ENCODER_HEVC_FORCE_PICTURE_TYPE,amf_int64(av1?AMF_VIDEO_ENCODER_AV1_FORCE_FRAME_TYPE_KEY:AMF_VIDEO_ENCODER_HEVC_PICTURE_TYPE_IDR)),"Force AMF keyframe");
            check(nv12->SetProperty(av1?AMF_VIDEO_ENCODER_AV1_FORCE_INSERT_SEQUENCE_HEADER:AMF_VIDEO_ENCODER_HEVC_INSERT_HEADER,true),"Force AMF stream headers");
        }
        last_qp=-1;
        const bool sample_stats=!av1 && submissions++%30==0;
        if(sample_stats)nv12->SetProperty(AMF_VIDEO_ENCODER_HEVC_STATISTICS_FEEDBACK,true);
        check(encoder->SubmitInput(nv12),"Submit AMF GPU encode");
        auto data=output(encoder);amf::AMFBufferPtr buffer(data);
        if(!buffer || buffer->GetMemoryType()!=amf::AMF_MEMORY_HOST || !buffer->GetNative() || !buffer->GetSize() || buffer->GetSize()>capacity || data->GetPts()!=surface->GetPts())throw std::runtime_error("Invalid AMF compressed output/timestamp");
        if(sample_stats){amf_int64 qp=-1;if(data->GetProperty(AMF_VIDEO_ENCODER_HEVC_STATISTIC_AVERAGE_QP,&qp)==AMF_OK && qp>=0 && qp<=51)last_qp=int(qp);}
        amf_int64 type=-1;check(data->GetProperty(av1?AMF_VIDEO_ENCODER_AV1_OUTPUT_FRAME_TYPE:AMF_VIDEO_ENCODER_HEVC_OUTPUT_DATA_TYPE,&type),"Read AMF keyframe metadata");
        is_key=av1?(type==AMF_VIDEO_ENCODER_AV1_OUTPUT_FRAME_TYPE_KEY || type==AMF_VIDEO_ENCODER_AV1_OUTPUT_FRAME_TYPE_INTRA_ONLY):(type==AMF_VIDEO_ENCODER_HEVC_OUTPUT_DATA_TYPE_IDR || type==AMF_VIDEO_ENCODER_HEVC_OUTPUT_DATA_TYPE_I);
        length=buffer->GetSize();std::memcpy(bytes,buffer->GetNative(),length);
    }
    ID3D11Texture2D* preview(){
        if(!ready)return nullptr;D3D11_TEXTURE2D_DESC desc{};input->GetDesc(&desc);ComPtr<ID3D11Texture2D> texture;
        hr(device->CreateTexture2D(&desc,nullptr,&texture),"Allocate AMF preview");context->CopyResource(texture.Get(),input.Get());wait_copy();return texture.Detach();
    }
};
}
extern "C" {
unsigned hc_amf_live_encoders() noexcept {return live_encoders.load();}
void hc_amf_size(void* handle,uint32_t* w,uint32_t* h) noexcept {if(handle && w && h){auto* e=static_cast<Encoder*>(handle);*w=e->width;*h=e->height;}}
// Optional output cap; input dimensions still describe the original GPU texture.
void* hc_amf_open_scaled(void* device,uint32_t w,uint32_t h,uint32_t fps,uint32_t bitrate,uint32_t av1,uint32_t profile,uint32_t max_w,uint32_t max_h,char* error,size_t capacity) noexcept {
    try{auto value=std::make_unique<Encoder>();value->open(static_cast<ID3D11Device*>(device),w,h,fps,bitrate,av1!=0,profile,max_w,max_h);return value.release();}
    catch(const std::exception& e){report(error,capacity,e.what());}catch(...){report(error,capacity,"Unknown encoder initialization error");}return nullptr;
}
void* hc_amf_open(void* device,uint32_t w,uint32_t h,uint32_t fps,uint32_t bitrate,uint32_t av1,char* error,size_t capacity) noexcept {
    try{auto value=std::make_unique<Encoder>();value->open(static_cast<ID3D11Device*>(device),w,h,fps,bitrate,av1!=0);return value.release();}
    catch(const std::exception& e){report(error,capacity,e.what());}catch(...){report(error,capacity,"Unknown AMF initialization error");}return nullptr;
}
// Profile is immutable for a session. Original ABI remains available to diagnostics.
void* hc_amf_open_profile(void* device,uint32_t w,uint32_t h,uint32_t fps,uint32_t bitrate,uint32_t av1,uint32_t profile,char* error,size_t capacity) noexcept {
    try{auto value=std::make_unique<Encoder>();value->open(static_cast<ID3D11Device*>(device),w,h,fps,bitrate,av1!=0,profile);return value.release();}
    catch(const std::exception& e){report(error,capacity,e.what());}catch(...){report(error,capacity,"Unknown AMF initialization error");}return nullptr;
}
void hc_amf_feedback(void* handle,int32_t* qp,uint32_t* bitrate,uint32_t* vbv,uint32_t* fps,uint32_t* runtime) noexcept {
    if(handle && qp && bitrate && vbv && fps && runtime){auto* e=static_cast<Encoder*>(handle);*qp=e->last_qp;*bitrate=e->applied_bitrate;*vbv=e->vbv_bits;*fps=e->fps;*runtime=e->vbv_runtime?1:0;}
}
int hc_amf_encode(void* handle,void* texture,uint64_t timestamp,uint32_t key,uint8_t* bytes,size_t capacity,size_t* length,uint32_t* is_key,char* error,size_t error_capacity) noexcept {
    try{if(!handle || !length || !is_key)throw std::runtime_error("Invalid AMF encode arguments");bool flag=false;*length=0;*is_key=0;static_cast<Encoder*>(handle)->encode(static_cast<ID3D11Texture2D*>(texture),timestamp,key!=0,bytes,capacity,*length,flag);*is_key=flag;return 0;}
    catch(const std::exception& e){report(error,error_capacity,e.what());}catch(...){report(error,error_capacity,"Unknown AMF encode error");}return -1;
}
int hc_amf_rates(void* handle,uint32_t bitrate,uint32_t fps,char* error,size_t capacity) noexcept {
    try{if(!handle)throw std::runtime_error("Invalid AMF handle");static_cast<Encoder*>(handle)->rates(bitrate,fps);return 0;}
    catch(const std::exception& e){report(error,capacity,e.what());}catch(...){report(error,capacity,"Unknown AMF rate-control error");}return 2;
}
void hc_amf_close(void* handle) noexcept {delete static_cast<Encoder*>(handle);}
void* hc_amf_preview(void* handle) noexcept {try{return handle?static_cast<Encoder*>(handle)->preview():nullptr;}catch(...){return nullptr;}}
}
