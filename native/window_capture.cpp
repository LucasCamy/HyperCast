// Free-threaded Windows Graphics Capture. Pixels stay on the selected GPU.
#define NOMINMAX
#include <windows.h>
#include <dwmapi.h>
#include <d3d11.h>
#include <dxgi1_2.h>
#include <windows.graphics.capture.interop.h>
#include <windows.graphics.directx.direct3d11.interop.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Graphics.Capture.h>
#include <winrt/Windows.Graphics.DirectX.Direct3D11.h>
#include <atomic>
#include <memory>
#include <algorithm>
#include <cstring>
#include <stdexcept>
using namespace winrt;
using namespace winrt::Windows::Graphics;
using namespace winrt::Windows::Graphics::Capture;
using namespace winrt::Windows::Graphics::DirectX;
using namespace winrt::Windows::Graphics::DirectX::Direct3D11;
static void report(char* out, size_t size, const char* text) noexcept {
    if (out && size) { auto n=std::min(size-1,strlen(text)); memcpy(out,text,n); out[n]=0; }
}
struct Apartment { Apartment(){init_apartment(apartment_type::multi_threaded);} ~Apartment(){uninit_apartment();} };
struct WindowCapture {
    Apartment apartment;
    HWND window; DWORD pid;
    com_ptr<ID3D11Device> device;
    com_ptr<ID3D11DeviceContext> context;
    IDirect3DDevice runtime_device{nullptr};
    GraphicsCaptureItem item{nullptr};
    Direct3D11CaptureFramePool pool{nullptr};
    GraphicsCaptureSession session{nullptr};
    SizeInt32 size{};
    std::shared_ptr<std::atomic_bool> closed=std::make_shared<std::atomic_bool>(false);
    event_token closed_token{};
    WindowCapture(HWND hwnd, DWORD process, UINT adapter_index):window(hwnd),pid(process) {
        validate();
        if (!GraphicsCaptureSession::IsSupported()) throw std::runtime_error("Windows Graphics Capture unavailable");
        com_ptr<IDXGIFactory1> factory; check_hresult(CreateDXGIFactory1(__uuidof(IDXGIFactory1),factory.put_void()));
        com_ptr<IDXGIAdapter1> adapter; check_hresult(factory->EnumAdapters1(adapter_index,adapter.put()));
        D3D_FEATURE_LEVEL level; D3D_FEATURE_LEVEL levels[]={D3D_FEATURE_LEVEL_11_1,D3D_FEATURE_LEVEL_11_0};
        check_hresult(D3D11CreateDevice(adapter.get(),D3D_DRIVER_TYPE_UNKNOWN,nullptr,D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            levels,2,D3D11_SDK_VERSION,device.put(),&level,context.put()));
        auto dxgi=device.as<IDXGIDevice>(); com_ptr<IInspectable> inspectable;
        check_hresult(CreateDirect3D11DeviceFromDXGIDevice(dxgi.get(),inspectable.put()));
        runtime_device=inspectable.as<IDirect3DDevice>();
        auto interop=get_activation_factory<GraphicsCaptureItem,IGraphicsCaptureItemInterop>();
        check_hresult(interop->CreateForWindow(hwnd,guid_of<GraphicsCaptureItem>(),put_abi(item)));
        size=item.Size(); validate_size(size);
        auto flag=closed;
        closed_token=item.Closed([flag](auto const&,auto const&){flag->store(true);});
        pool=Direct3D11CaptureFramePool::CreateFreeThreaded(runtime_device,DirectXPixelFormat::B8G8R8A8UIntNormalized,2,size);
        session=pool.CreateCaptureSession(item);
        // Keep the Windows capture indicator; do not silently capture other windows.
        session.StartCapture();
    }
    ~WindowCapture() noexcept {
        try { if(item) item.Closed(closed_token); } catch (...) {}
        try { if(session) session.Close(); } catch (...) {}
        try { if(pool) pool.Close(); } catch (...) {}
    }
    void validate(){ DWORD current=0; GetWindowThreadProcessId(window,&current);
        if(!IsWindow(window)||!pid||current!=pid||pid==GetCurrentProcessId()||closed->load())
            throw std::runtime_error("A janela selecionada foi fechada ou mudou de processo"); }
    static void validate_size(SizeInt32 s){ if(s.Width<2||s.Height<2||s.Width>3840||s.Height>2160)
        throw std::runtime_error("Janela deve ter entre 2x2 e 3840x2160 pixels"); }
    com_ptr<ID3D11Texture2D> acquire(){
        validate(); if(IsIconic(window)) return nullptr;
        auto frame=pool.TryGetNextFrame(); if(!frame) return nullptr;
        auto content=frame.ContentSize(); validate_size(content);
        auto access=frame.Surface().as<::Windows::Graphics::DirectX::Direct3D11::IDirect3DDxgiInterfaceAccess>();
        com_ptr<ID3D11Texture2D> source; check_hresult(access->GetInterface(__uuidof(ID3D11Texture2D),source.put_void()));
        D3D11_TEXTURE2D_DESC desc{}; source->GetDesc(&desc);
        UINT width=std::min(desc.Width,static_cast<UINT>(content.Width))&~1u;
        UINT height=std::min(desc.Height,static_cast<UINT>(content.Height))&~1u;
        if(!width||!height) return nullptr;
        desc.Width=width;desc.Height=height;desc.MipLevels=1;desc.ArraySize=1;desc.Usage=D3D11_USAGE_DEFAULT;
        desc.BindFlags=D3D11_BIND_SHADER_RESOURCE;desc.CPUAccessFlags=0;desc.MiscFlags=0;
        com_ptr<ID3D11Texture2D> snapshot;check_hresult(device->CreateTexture2D(&desc,nullptr,snapshot.put()));
        D3D11_BOX box{0,0,0,width,height,1};context->CopySubresourceRegion(snapshot.get(),0,0,0,0,source.get(),0,&box);
        D3D11_QUERY_DESC query_desc{D3D11_QUERY_EVENT,0};com_ptr<ID3D11Query> fence;
        check_hresult(device->CreateQuery(&query_desc,fence.put()));context->End(fence.get());context->Flush();
        ULONGLONG deadline=GetTickCount64()+100;
        for(;;){auto hr=context->GetData(fence.get(),nullptr,0,0);check_hresult(hr);if(hr==S_OK)break;
            if(GetTickCount64()>deadline)throw std::runtime_error("Window GPU copy timed out; rebuild GPU pipeline");SwitchToThread();}
        frame.Close();
        if(content.Width!=size.Width||content.Height!=size.Height){size=content;pool.Recreate(runtime_device,DirectXPixelFormat::B8G8R8A8UIntNormalized,2,size);}
        return snapshot;
    }
};
extern "C" {
void* hc_window_open(size_t hwnd, unsigned pid, unsigned adapter, char* error,size_t capacity) noexcept {
    try{return new WindowCapture(reinterpret_cast<HWND>(hwnd),pid,adapter);}catch(hresult_error const& e){report(error,capacity,to_string(e.message()).c_str());}catch(std::exception const& e){report(error,capacity,e.what());}catch(...){report(error,capacity,"Window capture failed");}return nullptr;
}
void* hc_window_device(void* handle) noexcept {auto* c=static_cast<WindowCapture*>(handle);c->device->AddRef();return c->device.get();}
void hc_window_size(void* handle,unsigned* w,unsigned* h) noexcept {auto* c=static_cast<WindowCapture*>(handle);*w=c->size.Width&~1u;*h=c->size.Height&~1u;}
int hc_window_frame(void* handle,void** texture,char* error,size_t capacity) noexcept {
    *texture=nullptr;try{auto frame=static_cast<WindowCapture*>(handle)->acquire();*texture=frame.detach();return 0;}
    catch(hresult_error const& e){report(error,capacity,to_string(e.message()).c_str());}catch(std::exception const& e){report(error,capacity,e.what());}catch(...){report(error,capacity,"Window capture failed");}return -1;
}
void hc_window_close(void* handle) noexcept {delete static_cast<WindowCapture*>(handle);}
// Bounded metadata enumeration; never read window content or log titles.
typedef void (*WindowCallback)(size_t,unsigned,const wchar_t*,void*);
struct Enumeration{WindowCallback callback;void* data;};
BOOL CALLBACK enumerate(HWND hwnd,LPARAM param){
    DWORD pid=0;GetWindowThreadProcessId(hwnd,&pid);DWORD cloaked=0;DwmGetWindowAttribute(hwnd,DWMWA_CLOAKED,&cloaked,sizeof(cloaked));
    if(!IsWindowVisible(hwnd)||cloaked||pid==GetCurrentProcessId()||GetWindow(hwnd,GW_OWNER))return TRUE;
    wchar_t title[512]{};if(!GetWindowTextW(hwnd,title,512))return TRUE;
    auto* e=reinterpret_cast<Enumeration*>(param);e->callback(reinterpret_cast<size_t>(hwnd),pid,title,e->data);return TRUE;
}
void hc_window_list(WindowCallback callback,void* data) noexcept {Enumeration e{callback,data};EnumWindows(enumerate,reinterpret_cast<LPARAM>(&e));}
int hc_window_is_live(size_t window,unsigned pid) noexcept {
    HWND hwnd=reinterpret_cast<HWND>(window);DWORD actual=0;
    return IsWindow(hwnd)&&GetWindowThreadProcessId(hwnd,&actual)&&actual==pid;
}
// Local DWM preview only. No readback, screenshots, files or network traffic.
struct WindowPreview {
    HWND source=nullptr,destination=nullptr;DWORD pid=0;HTHUMBNAIL thumbnail=nullptr;
    ~WindowPreview(){if(thumbnail)DwmUnregisterThumbnail(thumbnail);}
    void validate(){
        DWORD actual=0;GetWindowThreadProcessId(source,&actual);
        DWORD destination_pid=0;GetWindowThreadProcessId(destination,&destination_pid);
        if(!IsWindow(source)||actual!=pid||destination_pid!=GetCurrentProcessId())throw std::runtime_error("Preview window closed or changed");
    }
};
void* hc_window_preview_open(size_t destination,size_t source,unsigned pid) noexcept {
    try {
        auto p=std::make_unique<WindowPreview>();p->source=reinterpret_cast<HWND>(source);
        p->destination=reinterpret_cast<HWND>(destination);p->pid=pid;p->validate();
        if(IsIconic(p->source))return nullptr;
        check_hresult(DwmRegisterThumbnail(p->destination,p->source,&p->thumbnail));return p.release();
    }catch(...){return nullptr;}
}
int hc_window_preview_present(void* handle,int x,int y,int width,int height) noexcept {
    try {
        if(!handle||width<=0||height<=0||width>4096||height>4096||x<0||y<0||x>32768||y>32768)return -1;
        auto* p=static_cast<WindowPreview*>(handle);p->validate();
        if(IsIconic(p->source))return -1;
        SIZE size{};check_hresult(DwmQueryThumbnailSourceSize(p->thumbnail,&size));
        RECT client{};
        if(GetClientRect(p->source,&client)){size.cx=client.right-client.left;size.cy=client.bottom-client.top;}
        if(size.cx<=0||size.cy<=0||size.cx>32768||size.cy>32768)return -1;
        const double scale=std::min(double(width)/size.cx,double(height)/size.cy);
        const int w=std::max(1,int(size.cx*scale)),h=std::max(1,int(size.cy*scale));
        DWM_THUMBNAIL_PROPERTIES properties{};
        properties.dwFlags=DWM_TNP_RECTDESTINATION|DWM_TNP_VISIBLE|DWM_TNP_OPACITY|DWM_TNP_SOURCECLIENTAREAONLY;
        properties.rcDestination={x+(width-w)/2,y+(height-h)/2,x+(width+w)/2,y+(height+h)/2};
        properties.fVisible=TRUE;properties.opacity=255;properties.fSourceClientAreaOnly=TRUE;
        check_hresult(DwmUpdateThumbnailProperties(p->thumbnail,&properties));return 0;
    }catch(...){return -1;}
}
void hc_window_preview_close(void* handle) noexcept {delete static_cast<WindowPreview*>(handle);}
}
