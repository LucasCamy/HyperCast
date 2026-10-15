// Synthetic HDR pixels only. Test-only readback verifies the GPU color pass.
#define NOMINMAX
#include "../native/capture_color.h"
#include <array>
#include <cstdio>
#include <memory>
#include <vector>

extern "C" {
void* hc_nvenc_open(void*, uint32_t, uint32_t, uint32_t, uint32_t, uint32_t, char*, size_t) noexcept;
int hc_nvenc_encode(void*, void*, uint64_t, uint32_t, uint8_t*, size_t, size_t*, uint32_t*, char*, size_t) noexcept;
void hc_nvenc_close(void*) noexcept;
int hc_nvenc_rates(void*,uint32_t,uint32_t,char*,size_t) noexcept;
}

int main() {
    using Microsoft::WRL::ComPtr;
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> context;
    auto require = [](bool value, const char* message) { if (!value) throw std::runtime_error(message); };
    try {
        require(SUCCEEDED(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr,
            D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION, &device, nullptr, &context)), "Hardware D3D11 device");
        // black, linear quarter-white, SDR white, HDR highlight in binary16.
        std::array<unsigned short, 32> pixels{};
        const unsigned short levels[] = {0, 0x3400, 0x3c00, 0x4400};
        for (int y = 0; y < 2; ++y) for (int x = 0; x < 4; ++x) {
            for (int c = 0; c < 3; ++c) pixels[(y * 4 + x) * 4 + c] = levels[x];
            pixels[(y * 4 + x) * 4 + 3] = 0x3c00;
        }
        D3D11_TEXTURE2D_DESC desc{};
        desc.Width = 4; desc.Height = 2; desc.MipLevels = 1; desc.ArraySize = 1;
        desc.Format = DXGI_FORMAT_R16G16B16A16_FLOAT; desc.SampleDesc.Count = 1;
        desc.Usage = D3D11_USAGE_DEFAULT;
        D3D11_SUBRESOURCE_DATA data{pixels.data(), 32, 0};
        ComPtr<ID3D11Texture2D> source, destination, readback;
        require(SUCCEEDED(device->CreateTexture2D(&desc, &data, &source)), "HDR source");
        desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM; desc.BindFlags = D3D11_BIND_RENDER_TARGET;
        require(SUCCEEDED(device->CreateTexture2D(&desc, nullptr, &destination)), "SDR destination");
        desc.Usage = D3D11_USAGE_STAGING; desc.BindFlags = 0; desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
        require(SUCCEEDED(device->CreateTexture2D(&desc, nullptr, &readback)), "Synthetic readback");
        CaptureColor converter;
        for (int iteration = 0; iteration < 3; ++iteration) {
            converter.convert(device.Get(), context.Get(), source.Get(), destination.Get());
            context->CopyResource(readback.Get(), destination.Get());
            D3D11_MAPPED_SUBRESOURCE mapped{};
            require(SUCCEEDED(context->Map(readback.Get(), 0, D3D11_MAP_READ, 0, &mapped)), "Map synthetic output");
            bool valid = true;
            for (int y = 0; y < 2; ++y) {
                auto row = static_cast<unsigned char*>(mapped.pData) + y * mapped.RowPitch;
                valid &= row[0] == 0 && row[4] >= 135 && row[4] <= 139;
                valid &= row[8] > row[4] && row[12] > row[8] && row[12] <= 255;
                for (int x = 0; x < 4; ++x) valid &= row[4*x] == row[4*x+1] && row[4*x] == row[4*x+2] && row[4*x+3] == 255;
            }
            context->Unmap(readback.Get(), 0);
            require(valid, "GPU SDR output preserves black, sRGB midtones, highlight order and opacity");
        }
        std::puts("PASS: FP16 HDR GPU conversion, sRGB midtones, highlight compression, repeated frames");
        // Exercise the production bridge with HDR -> SDR -> HDR -> repeat.
        // These are generated solid-color textures, never desktop contents.
        desc.Width = 256; desc.Height = 256; desc.Format = DXGI_FORMAT_R16G16B16A16_FLOAT;
        desc.Usage = D3D11_USAGE_DEFAULT; desc.CPUAccessFlags = 0; desc.BindFlags = 0;
        std::vector<unsigned short> hdr_pixels(256 * 256 * 4, 0x3400);
        D3D11_SUBRESOURCE_DATA hdr_data{hdr_pixels.data(), 256 * 8, 0};
        ComPtr<ID3D11Texture2D> hdr_source, sdr_source;
        require(SUCCEEDED(device->CreateTexture2D(&desc, &hdr_data, &hdr_source)), "NVENC HDR source");
        desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
        std::vector<uint32_t> sdr_pixels(256 * 256, 0xff808080);
        D3D11_SUBRESOURCE_DATA sdr_data{sdr_pixels.data(), 256 * 4, 0};
        require(SUCCEEDED(device->CreateTexture2D(&desc, &sdr_data, &sdr_source)), "NVENC SDR source");
        std::array<char, 1024> error{};
        std::unique_ptr<void, decltype(&hc_nvenc_close)> encoder(
            hc_nvenc_open(device.Get(), 256, 256, 120, 40000000, 0, error.data(), error.size()), hc_nvenc_close);
        require(encoder != nullptr, error.data());
        std::vector<uint8_t> bytes(1024 * 1024);
        ID3D11Texture2D* sequence[] = {hdr_source.Get(), sdr_source.Get(), hdr_source.Get(), nullptr};
        for (int i = 0; i < 4; ++i) {
            size_t length = 0; uint32_t key = 0;
            require(hc_nvenc_encode(encoder.get(), sequence[i], i * 8333, 1, bytes.data(), bytes.size(),
                &length, &key, error.data(), error.size()) == 0, error.data());
            require(length > 0 && key == 1, "NVENC encoded access unit");
        }
        std::puts("PASS: production NVENC bridge encodes HDR/SDR transitions and retained GPU frame");
        for(unsigned i=4;i<12;++i){
            require(hc_nvenc_rates(encoder.get(),28000000,120,error.data(),error.size())==0,error.data());
            size_t length=0;uint32_t key=0;
            require(hc_nvenc_encode(encoder.get(),sdr_source.Get(),i*8333,i==11,bytes.data(),bytes.size(),&length,&key,error.data(),error.size())==0,error.data());
            require(length>0 && key==uint32_t(i==11),"Rate-only NVENC update must preserve GOP; requested key must work");
        }
        std::puts("PASS: NVENC rate changes and duplicates preserve GOP, explicit keyframe honored");
        return 0;
    } catch (const std::exception& error) { std::fprintf(stderr, "%s\n", error.what()); return 1; }
}
