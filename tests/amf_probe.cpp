// Generates GPU colors only: never captures a desktop or records user content.
#define NOMINMAX
#include <windows.h>
#include <d3d11.h>
#include <dxgi.h>
#include <wrl/client.h>
#include <chrono>
#include <cstdio>
#include <fstream>
#include <stdexcept>
#include <vector>
using Microsoft::WRL::ComPtr;
extern "C" {
void* hc_amf_open(void*,uint32_t,uint32_t,uint32_t,uint32_t,uint32_t,char*,size_t) noexcept;
int hc_amf_encode(void*,void*,uint64_t,uint32_t,uint8_t*,size_t,size_t*,uint32_t*,char*,size_t) noexcept;
int hc_amf_rates(void*,uint32_t,uint32_t,char*,size_t) noexcept;
void hc_amf_close(void*) noexcept;
void* hc_amf_preview(void*) noexcept;
}
void hr(HRESULT r){if(FAILED(r))throw std::runtime_error("Probe D3D11 failure");}
int main(int argc,char** argv){
    void* handle=nullptr;
    try{
        if(argc!=2)throw std::runtime_error("Expected synthetic output path");
        ComPtr<IDXGIFactory1> factory;hr(CreateDXGIFactory1(IID_PPV_ARGS(&factory)));
        ComPtr<IDXGIAdapter1> amd;
        for(UINT i=0;;++i){ComPtr<IDXGIAdapter1> adapter;if(factory->EnumAdapters1(i,&adapter)==DXGI_ERROR_NOT_FOUND)break;
            DXGI_ADAPTER_DESC1 desc{};hr(adapter->GetDesc1(&desc));if(desc.VendorId==0x1002){amd=adapter;std::printf("AMD adapter: %u\n",i);break;}}
        if(!amd)throw std::runtime_error("No AMD adapter available for hardware probe");
        ComPtr<ID3D11Device> device;ComPtr<ID3D11DeviceContext> context;
        hr(D3D11CreateDevice(amd.Get(),D3D_DRIVER_TYPE_UNKNOWN,nullptr,D3D11_CREATE_DEVICE_BGRA_SUPPORT,nullptr,0,D3D11_SDK_VERSION,&device,nullptr,&context));
        D3D11_TEXTURE2D_DESC desc{};desc.Width=1280;desc.Height=720;desc.MipLevels=1;desc.ArraySize=1;desc.Format=DXGI_FORMAT_B8G8R8A8_UNORM;desc.SampleDesc.Count=1;desc.BindFlags=D3D11_BIND_RENDER_TARGET|D3D11_BIND_SHADER_RESOURCE;
        ComPtr<ID3D11Texture2D> texture;hr(device->CreateTexture2D(&desc,nullptr,&texture));
        ComPtr<ID3D11RenderTargetView> target;hr(device->CreateRenderTargetView(texture.Get(),nullptr,&target));
        auto hdr_desc=desc;hdr_desc.Format=DXGI_FORMAT_R16G16B16A16_FLOAT;
        ComPtr<ID3D11Texture2D> hdr;ComPtr<ID3D11RenderTargetView> hdr_target;
        hr(device->CreateTexture2D(&hdr_desc,nullptr,&hdr));hr(device->CreateRenderTargetView(hdr.Get(),nullptr,&hdr_target));
        char error[1024]{};handle=hc_amf_open(device.Get(),1280,720,120,40000000,0,error,sizeof(error));
        if(!handle)throw std::runtime_error(error);
        std::ofstream file(argv[1],std::ios::binary|std::ios::trunc);if(!file)throw std::runtime_error("Cannot create synthetic test output");
        std::vector<uint8_t> bytes(16*1024*1024);unsigned keys=0;
        auto start=std::chrono::steady_clock::now();
        for(unsigned i=0;i<120;++i){
            float color[]{(i%30)/30.0f,0.25f,0.5f,1.0f};context->ClearRenderTargetView(target.Get(),color);
            context->ClearRenderTargetView(hdr_target.Get(),color);
            if(i==60 && hc_amf_rates(handle,28000000,60,error,sizeof(error)))throw std::runtime_error(error);
            size_t length=0;uint32_t key=0;
            if(hc_amf_encode(handle,i==119?nullptr:(i==30?hdr.Get():texture.Get()),uint64_t(i)*8333,i%30==0,bytes.data(),bytes.size(),&length,&key,error,sizeof(error)))throw std::runtime_error(error);
            if(i%30==0 && !key)throw std::runtime_error("AMF did not honor requested keyframe");
            keys+=key;file.write(reinterpret_cast<const char*>(bytes.data()),length);
        }
        ComPtr<ID3D11Texture2D> preview;preview.Attach(static_cast<ID3D11Texture2D*>(hc_amf_preview(handle)));
        if(!preview)throw std::runtime_error("AMF GPU preview unavailable");
        hc_amf_close(handle);handle=nullptr;file.close();
        double elapsed=std::chrono::duration<double>(std::chrono::steady_clock::now()-start).count();
        std::printf("PASS: AMD AMF HEVC 120 synthetic frames, %u keyframes, rates, repeat and GPU preview; %.2f ms/frame\n",keys,elapsed*1000/120);
        handle=hc_amf_open(device.Get(),1280,720,120,40000000,1,error,sizeof(error));
        if(handle){hc_amf_close(handle);handle=nullptr;std::puts("AV1 initializes on this AMD adapter; HEVC probe did not validate AV1 frames");}
        else std::puts("AV1 unavailable on this AMD adapter; HEVC remains available");
        return 0;
    }catch(const std::exception& e){if(handle)hc_amf_close(handle);std::fprintf(stderr,"%s\n",e.what());return 1;}
}
