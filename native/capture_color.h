// scRGB desktop duplication -> SDR BGRA, entirely on the capture GPU.
// A private sampleable copy is necessary: DXGI surfaces need not have SRV binding.
#pragma once
#include <d3d11.h>
#include <d3d10.h>
#include <d3dcompiler.h>
#include <wrl/client.h>
#include <cstring>
#include <stdexcept>
#include <string>

class CaptureColor {
    Microsoft::WRL::ComPtr<ID3D11Texture2D> sample;
    Microsoft::WRL::ComPtr<ID3D11ShaderResourceView> view;
    Microsoft::WRL::ComPtr<ID3D11RenderTargetView> target;
    Microsoft::WRL::ComPtr<ID3D11VertexShader> vertex;
    Microsoft::WRL::ComPtr<ID3D11PixelShader> pixel;
    Microsoft::WRL::ComPtr<ID3D11RasterizerState> raster;
    Microsoft::WRL::ComPtr<ID3D11SamplerState> sampler;
    Microsoft::WRL::ComPtr<ID3D11Buffer> parameters;
    static void check(HRESULT result, const char* operation) {
        if (FAILED(result)) throw std::runtime_error(std::string(operation) + ": HRESULT " + std::to_string(result));
    }
public:
    void convert(ID3D11Device* device, ID3D11DeviceContext* context,
                 ID3D11Texture2D* source, ID3D11Texture2D* destination) {
        Microsoft::WRL::ComPtr<ID3D10Multithread> protect;
        check(context->QueryInterface(IID_PPV_ARGS(&protect)), "Capture context protection");
        protect->SetMultithreadProtected(TRUE);
        protect->Enter();
        struct Unlock { ID3D10Multithread* lock; ~Unlock() { lock->Leave(); } } unlock{protect.Get()};
        D3D11_TEXTURE2D_DESC source_desc{},output_desc{},cached_desc{};
        source->GetDesc(&source_desc);destination->GetDesc(&output_desc);
        if(sample){sample->GetDesc(&cached_desc);if(cached_desc.Width!=source_desc.Width || cached_desc.Height!=source_desc.Height || cached_desc.Format!=source_desc.Format){
            sample.Reset();view.Reset();target.Reset();vertex.Reset();pixel.Reset();raster.Reset();sampler.Reset();parameters.Reset();
        }}
        if (!sample) {
            static constexpr char shader[] = R"(
                float4 vs(uint id : SV_VertexID) : SV_Position {
                    float2 p = float2((id << 1) & 2, id & 2);
                    return float4(p * float2(2, -2) + float2(-1, 1), 0, 1);
                }
                Texture2D<float4> image : register(t0);
                SamplerState linear_sampler : register(s0);
                cbuffer Params : register(b0) {float2 inverse_size; float hdr; float native_pixels;}
                float4 ps(float4 position : SV_Position) : SV_Target {
                    uint sw,sh; image.GetDimensions(sw,sh);
                    float3 sampled = native_pixels>0.5
                        ? image.Load(int3(min(int2(position.xy),int2(sw-1,sh-1)),0)).rgb
                        : image.Sample(linear_sampler, position.xy * inverse_size).rgb;
                    if(hdr<0.5) return float4(sampled,1);
                    float3 rgb = max(sampled,0);
                    // scRGB is linear Rec.709; preserve dark SDR values, compress highlights.
                    float luminance = dot(rgb, float3(0.2126, 0.7152, 0.0722));
                    float shoulder = max(luminance - 0.75, 0);
                    float mapped = luminance <= 0.75 ? luminance : 0.75 + 0.25 * shoulder / (shoulder + 0.25);
                    rgb = saturate(rgb * mapped / max(luminance, 0.000001));
                    float3 low = 12.92 * rgb;
                    float3 high = 1.055 * pow(rgb, 1.0 / 2.4) - 0.055;
                    return float4(rgb.r <= 0.0031308 ? low.r : high.r,
                                  rgb.g <= 0.0031308 ? low.g : high.g,
                                  rgb.b <= 0.0031308 ? low.b : high.b, 1);
                }
            )";
            Microsoft::WRL::ComPtr<ID3DBlob> vs, ps, errors;
            check(D3DCompile(shader, std::strlen(shader), nullptr, nullptr, nullptr, "vs", "vs_5_0",
                             D3DCOMPILE_ENABLE_STRICTNESS, 0, &vs, &errors), "Compile capture vertex shader");
            check(D3DCompile(shader, std::strlen(shader), nullptr, nullptr, nullptr, "ps", "ps_5_0",
                             D3DCOMPILE_ENABLE_STRICTNESS, 0, &ps, &errors), "Compile capture SDR shader");
            check(device->CreateVertexShader(vs->GetBufferPointer(), vs->GetBufferSize(), nullptr, &vertex), "Create capture vertex shader");
            check(device->CreatePixelShader(ps->GetBufferPointer(), ps->GetBufferSize(), nullptr, &pixel), "Create capture SDR shader");
            D3D11_TEXTURE2D_DESC desc{}; source->GetDesc(&desc);
            desc.Usage = D3D11_USAGE_DEFAULT; desc.BindFlags = D3D11_BIND_SHADER_RESOURCE;
            desc.CPUAccessFlags = 0; desc.MiscFlags = 0;
            check(device->CreateTexture2D(&desc, nullptr, &sample), "Allocate HDR GPU sample");
            check(device->CreateShaderResourceView(sample.Get(), nullptr, &view), "Create HDR sample view");
            check(device->CreateRenderTargetView(destination, nullptr, &target), "Create SDR target view");
            D3D11_RASTERIZER_DESC state{};
            state.FillMode = D3D11_FILL_SOLID; state.CullMode = D3D11_CULL_NONE; state.DepthClipEnable = TRUE;
            check(device->CreateRasterizerState(&state, &raster), "Create capture raster state");
            D3D11_SAMPLER_DESC sampling{};sampling.Filter=D3D11_FILTER_MIN_MAG_MIP_LINEAR;
            sampling.AddressU=sampling.AddressV=sampling.AddressW=D3D11_TEXTURE_ADDRESS_CLAMP;sampling.MaxLOD=D3D11_FLOAT32_MAX;
            check(device->CreateSamplerState(&sampling,&sampler),"Create GPU scaling sampler");
            D3D11_BUFFER_DESC buffer{};buffer.ByteWidth=16;buffer.BindFlags=D3D11_BIND_CONSTANT_BUFFER;
            check(device->CreateBuffer(&buffer,nullptr,&parameters),"Create capture conversion parameters");
        }
        D3D11_TEXTURE2D_DESC desc{}; destination->GetDesc(&desc);
        context->CopyResource(sample.Get(), source);
        ID3D11RenderTargetView* render_target = target.Get();
        ID3D11ShaderResourceView* resource = view.Get();
        D3D11_VIEWPORT viewport{0, 0, static_cast<float>(desc.Width), static_cast<float>(desc.Height), 0, 1};
        context->ClearState();
        context->RSSetState(raster.Get()); context->RSSetViewports(1, &viewport);
        context->OMSetRenderTargets(1, &render_target, nullptr);
        context->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        context->VSSetShader(vertex.Get(), nullptr, 0);
        context->PSSetShader(pixel.Get(), nullptr, 0);
        context->PSSetShaderResources(0, 1, &resource);
        const float values[4]={1.0f/desc.Width,1.0f/desc.Height,source_desc.Format==DXGI_FORMAT_R16G16B16A16_FLOAT?1.0f:0.0f,
            desc.Width>=source_desc.Width && desc.Height>=source_desc.Height && desc.Width-source_desc.Width<=1 && desc.Height-source_desc.Height<=1?1.0f:0.0f};
        context->UpdateSubresource(parameters.Get(),0,nullptr,values,0,0);
        ID3D11Buffer* buffer=parameters.Get();ID3D11SamplerState* sampling=sampler.Get();
        context->PSSetConstantBuffers(0,1,&buffer);context->PSSetSamplers(0,1,&sampling);
        context->Draw(3, 0);
        context->ClearState(); // unbind SRV/RTV before NVENC maps the destination
    }
};
