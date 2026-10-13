// Child-window D3D11VA presenter: decoded NV12/P010 surface -> GPU video
// processor -> flip-discard swapchain. No Map/readback and no CPU RGB buffer.
#define NOMINMAX
#include <windows.h>
#include <d3d11.h>
#include <d3d10.h>
#include <dxgi1_3.h>
#include <wrl/client.h>
#include <algorithm>
#include <stdexcept>
#include <string>
#include <cstring>
#include <memory>
using Microsoft::WRL::ComPtr;
static void hr(HRESULT result, const char* where) { if (FAILED(result)) throw std::runtime_error(std::string(where) + ": " + std::to_string(result)); }
struct Presenter {
    HWND window = nullptr;
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> context;
    ComPtr<ID3D11VideoDevice> video;
    ComPtr<ID3D11VideoContext> video_context;
    ComPtr<IDXGISwapChain1> chain;
    ComPtr<ID3D11VideoProcessorEnumerator> enumerator;
    ComPtr<ID3D11VideoProcessor> processor;
    ComPtr<ID3D11VideoProcessorOutputView> output;
    ComPtr<ID3D11Query> fence;
    uint32_t width = 0, height = 0, source_width = 0, source_height = 0;
    bool ready = false;
    explicit Presenter(HWND parent) {
        window = CreateWindowExW(0, L"STATIC", L"", WS_CHILD | WS_CLIPSIBLINGS, 0, 0, 1, 1, parent, nullptr, GetModuleHandleW(nullptr), nullptr);
        if (!window) throw std::runtime_error("Cannot create DirectX video canvas");
    }
    ~Presenter() { if (window) DestroyWindow(window); }
    void hide() { ShowWindow(window, SW_HIDE); }
    void present(ID3D11Texture2D* texture, uint32_t slice, uint32_t frame_width, uint32_t frame_height, int x, int y, uint32_t w, uint32_t h) {
        if (!texture || !w || !h || w > 8192 || h > 8192 || !frame_width || !frame_height) { hide(); return; }
        D3D11_TEXTURE2D_DESC input_desc{}; texture->GetDesc(&input_desc);
        if (slice >= input_desc.ArraySize || frame_width > input_desc.Width || frame_height > input_desc.Height) throw std::runtime_error("Invalid decoded texture geometry");
        ComPtr<ID3D11Device> owner; texture->GetDevice(&owner);
        if (device && owner.Get() != device.Get()) {
            ready = false;
            // A track can resubscribe with a fresh D3D11VA device.
            output.Reset(); processor.Reset(); enumerator.Reset(); chain.Reset(); fence.Reset();
            video_context.Reset(); video.Reset(); context.Reset(); device.Reset();
        }
        if (!ready) {
            // A failed partial initialization must be retryable without using
            // a null chain/context/query on the following frame.
            output.Reset(); processor.Reset(); enumerator.Reset(); chain.Reset(); fence.Reset();
            video_context.Reset(); video.Reset(); context.Reset(); device.Reset();
            texture->GetDevice(&device); device->GetImmediateContext(&context);
            ComPtr<ID3D10Multithread> protect; hr(context.As(&protect), "Multithread protection"); protect->SetMultithreadProtected(TRUE);
            hr(device.As(&video), "D3D11VideoDevice"); hr(context.As(&video_context), "D3D11VideoContext");
            ComPtr<IDXGIDevice> dxgi; hr(device.As(&dxgi), "DXGI device");
            ComPtr<IDXGIAdapter> adapter; hr(dxgi->GetAdapter(&adapter), "GPU adapter");
            ComPtr<IDXGIFactory2> factory; hr(adapter->GetParent(IID_PPV_ARGS(&factory)), "DXGI factory");
            DXGI_SWAP_CHAIN_DESC1 desc{};
            desc.Width = w; desc.Height = h; desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
            desc.SampleDesc.Count = 1; desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
            desc.BufferCount = 2; desc.SwapEffect = DXGI_SWAP_EFFECT_FLIP_DISCARD;
            hr(factory->CreateSwapChainForHwnd(device.Get(), window, &desc, nullptr, nullptr, &chain), "Flip-discard swapchain");
            ComPtr<IDXGIDevice1> latency; if (SUCCEEDED(device.As(&latency))) latency->SetMaximumFrameLatency(1);
            width = w; height = h;
            D3D11_QUERY_DESC query{D3D11_QUERY_EVENT, 0};
            hr(device->CreateQuery(&query, &fence), "Presentation fence");
            ready = true;
        }
        if (width != w || height != h) {
            output.Reset(); processor.Reset(); enumerator.Reset();
            hr(chain->ResizeBuffers(2, w, h, DXGI_FORMAT_B8G8R8A8_UNORM, 0), "Resize video canvas");
            width = w; height = h;
        }
        if (!processor || source_width != frame_width || source_height != frame_height) {
            output.Reset(); processor.Reset(); enumerator.Reset();
            D3D11_VIDEO_PROCESSOR_CONTENT_DESC desc{};
            desc.InputFrameFormat = D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE;
            desc.InputWidth = frame_width; desc.InputHeight = frame_height;
            desc.OutputWidth = w; desc.OutputHeight = h;
            desc.InputFrameRate.Numerator = 120; desc.InputFrameRate.Denominator = 1;
            desc.OutputFrameRate = desc.InputFrameRate; desc.Usage = D3D11_VIDEO_USAGE_PLAYBACK_NORMAL;
            hr(video->CreateVideoProcessorEnumerator(&desc, &enumerator), "Video processor enumerator");
            hr(video->CreateVideoProcessor(enumerator.Get(), 0, &processor), "Video processor");
            source_width = frame_width; source_height = frame_height;
        }
        // Flip-discard rotates buffers: get the current output buffer per frame.
        output.Reset();
        ComPtr<ID3D11Texture2D> back; hr(chain->GetBuffer(0, IID_PPV_ARGS(&back)), "Swapchain buffer");
        D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC out_desc{};
        out_desc.ViewDimension = D3D11_VPOV_DIMENSION_TEXTURE2D;
        hr(video->CreateVideoProcessorOutputView(back.Get(), enumerator.Get(), &out_desc, &output), "Video output view");
        D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC in_desc{};
        in_desc.ViewDimension = D3D11_VPIV_DIMENSION_TEXTURE2D; in_desc.Texture2D.ArraySlice = slice;
        ComPtr<ID3D11VideoProcessorInputView> input;
        hr(video->CreateVideoProcessorInputView(texture, enumerator.Get(), &in_desc, &input), "Decoded texture input view");
        RECT source{0, 0, static_cast<LONG>(frame_width), static_cast<LONG>(frame_height)};
        const double scale = std::min(double(w) / frame_width, double(h) / frame_height);
        LONG dw = static_cast<LONG>(frame_width * scale), dh = static_cast<LONG>(frame_height * scale);
        RECT dest{(LONG(w) - dw) / 2, (LONG(h) - dh) / 2, (LONG(w) + dw) / 2, (LONG(h) + dh) / 2};
        RECT full{0, 0, static_cast<LONG>(w), static_cast<LONG>(h)};
        video_context->VideoProcessorSetStreamSourceRect(processor.Get(), 0, TRUE, &source);
        video_context->VideoProcessorSetStreamDestRect(processor.Get(), 0, TRUE, &dest);
        video_context->VideoProcessorSetOutputTargetRect(processor.Get(), TRUE, &full);
        D3D11_VIDEO_COLOR black{}; video_context->VideoProcessorSetOutputBackgroundColor(processor.Get(), FALSE, &black);
        D3D11_VIDEO_PROCESSOR_STREAM stream{}; stream.Enable = TRUE; stream.pInputSurface = input.Get();
        hr(video_context->VideoProcessorBlt(processor.Get(), output.Get(), 0, 1, &stream), "GPU NV12 presentation");
        // Fence before dropping the AVFrame: its decoder-array slot must not be
        // recycled while the video processor is still reading it.
        context->End(fence.Get()); context->Flush();
        auto start = GetTickCount64();
        for (;;) {
            HRESULT ready = context->GetData(fence.Get(), nullptr, 0, D3D11_ASYNC_GETDATA_DONOTFLUSH); hr(ready, "Wait GPU presentation");
            if (ready == S_OK) break;
            if (GetTickCount64() - start > 1000) throw std::runtime_error("Presentation GPU timeout");
            Sleep(0);
        }
        SetWindowPos(window, nullptr, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        hr(chain->Present(0, 0), "Present video");
    }
};
extern "C" {
void* hc_presenter_open(void* parent) noexcept { try { return new Presenter(static_cast<HWND>(parent)); } catch (...) { return nullptr; } }
int hc_presenter_frame(void* handle, void* texture, uint32_t slice, uint32_t fw, uint32_t fh, int x, int y, uint32_t w, uint32_t h, char* error, size_t cap) noexcept {
    try { if (!handle) throw std::runtime_error("No presenter"); static_cast<Presenter*>(handle)->present(static_cast<ID3D11Texture2D*>(texture), slice, fw, fh, x, y, w, h); return 0; }
    catch (const std::exception& e) { if (error && cap) { size_t size = std::min(cap - 1, strlen(e.what())); memcpy(error, e.what(), size); error[size] = 0; } return -1; }
    catch (...) { return -1; }
}
void hc_presenter_hide(void* handle) noexcept { if (handle) static_cast<Presenter*>(handle)->hide(); }
void hc_presenter_close(void* handle) noexcept { delete static_cast<Presenter*>(handle); }
}
