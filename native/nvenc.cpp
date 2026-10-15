// Synchronous D3D11/NVENC bridge. Only compressed data crosses into Rust RAM.
// One GPU input surface, no CPU readback, B frames or lookahead. This initial
// path measures behavior before introducing an asynchronous bounded pool.
#define NOMINMAX
#include <windows.h>
#include <d3d11.h>
#include <dxgi.h>
#include <d3d10.h>
#include <wrl/client.h>
#include <nvEncodeAPI.h>
#include <algorithm>
#include <cstring>
#include <memory>
#include <atomic>
static std::atomic<unsigned> live_encoders{0};
#include <stdexcept>
#include <string>
#include <vector>
#include "capture_color.h"
#include "capture_size.h"

using Microsoft::WRL::ComPtr;

static void report(char* error, size_t capacity, const char* message) noexcept {
    if (error && capacity) {
        const size_t length = std::min(capacity - 1, std::strlen(message));
        std::memcpy(error, message, length);
        error[length] = 0;
    }
}

struct Encoder {
    Encoder(){++live_encoders;}
    HMODULE dll = nullptr;
    NV_ENCODE_API_FUNCTION_LIST api{};
    void* session = nullptr;
    NV_ENC_REGISTERED_PTR registered = nullptr;
    NV_ENC_INPUT_PTR mapped = nullptr;
    NV_ENC_OUTPUT_PTR output = nullptr;
    bool output_locked = false;
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> context;
    ComPtr<ID3D11Texture2D> input;
    ComPtr<ID3D11Query> copy_done;
    NV_ENC_CONFIG config{};
    NV_ENC_INITIALIZE_PARAMS init{};
    uint32_t width = 0, height = 0, fps = 0, source_width = 0, source_height = 0;
    bool input_ready = false;
    CaptureColor capture_color;

    ~Encoder() noexcept {
        --live_encoders;
        if (session) {
            if (output_locked) api.nvEncUnlockBitstream(session, output);
            if (mapped) api.nvEncUnmapInputResource(session, mapped);
            if (output) api.nvEncDestroyBitstreamBuffer(session, output);
            if (registered) api.nvEncUnregisterResource(session, registered);
            api.nvEncDestroyEncoder(session);
        }
        if (dll) FreeLibrary(dll);
    }

    void check(NVENCSTATUS status, const char* operation) {
        if (status != NV_ENC_SUCCESS) {
            std::string message = std::string(operation) + ": NVENC status " + std::to_string(status);
            if (session && api.nvEncGetLastErrorString) {
                const char* detail = api.nvEncGetLastErrorString(session);
                if (detail) message += std::string(" (") + detail + ")";
            }
            throw std::runtime_error(message);
        }
    }
    static void hr(HRESULT value, const char* operation) {
        if (FAILED(value)) throw std::runtime_error(std::string(operation) + ": HRESULT " + std::to_string(value));
    }

    void open(ID3D11Device* d, uint32_t w, uint32_t h, uint32_t f, uint32_t bitrate, bool av1) {
        if (!d || !hc_capture_size(w,h) || f < 1 || f > 240 || bitrate < 1000000 || bitrate > 45000000)
            throw std::runtime_error("Invalid NVENC configuration (max 8192 per axis and 32 megapixels, 1-240 FPS, 1-45 Mbps)");
        source_width=w;source_height=h;fps=f;device=d;
        ComPtr<IDXGIDevice> dxgi_device;
        ComPtr<IDXGIAdapter> adapter;
        DXGI_ADAPTER_DESC adapter_desc{};
        hr(device.As(&dxgi_device), "Query capture adapter");
        hr(dxgi_device->GetAdapter(&adapter), "Get capture adapter");
        hr(adapter->GetDesc(&adapter_desc), "Get capture adapter description");
        if (adapter_desc.VendorId != 0x10DE)
            throw std::runtime_error("The capture GPU is not NVIDIA. Select a monitor connected to NVIDIA; AMF is not available in this build.");
        device->GetImmediateContext(&context);
        dll = LoadLibraryExW(L"nvEncodeAPI64.dll", nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32);
        if (!dll) throw std::runtime_error("NVIDIA NVENC driver DLL not found in System32");
        auto maximum_version = reinterpret_cast<decltype(&NvEncodeAPIGetMaxSupportedVersion)>(GetProcAddress(dll, "NvEncodeAPIGetMaxSupportedVersion"));
        if (!maximum_version) throw std::runtime_error("NVENC driver version query unavailable; update the NVIDIA driver.");
        uint32_t driver_version = 0;
        check(maximum_version(&driver_version), "Query NVENC driver API version");
        const uint32_t required_version = (NVENCAPI_MAJOR_VERSION << 4) | NVENCAPI_MINOR_VERSION;
        if (driver_version < required_version)
            throw std::runtime_error("The NVIDIA driver does not support the NVENC API required by this build. Update the NVIDIA driver.");
        auto create = reinterpret_cast<NVENCSTATUS(NVENCAPI*)(NV_ENCODE_API_FUNCTION_LIST*)>(GetProcAddress(dll, "NvEncodeAPICreateInstance"));
        if (!create) throw std::runtime_error("NvEncodeAPICreateInstance unavailable");
        api.version = NV_ENCODE_API_FUNCTION_LIST_VER;
        check(create(&api), "Create NVENC API (driver 610+ required)");
        NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS open{};
        open.version = NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER;
        open.apiVersion = NVENCAPI_VERSION;
        open.device = d;
        open.deviceType = NV_ENC_DEVICE_TYPE_DIRECTX;
        check(api.nvEncOpenEncodeSessionEx(&open, &session), "Open D3D11 NVENC session (device must be NVIDIA)");
        const GUID codec = av1 ? NV_ENC_CODEC_AV1_GUID : NV_ENC_CODEC_HEVC_GUID;
        // Check the selected GPU, not its generation/name: decode support does not imply encode support.
        uint32_t codec_count = 0;
        check(api.nvEncGetEncodeGUIDCount(session, &codec_count), "Query supported encode codecs");
        if (!codec_count || codec_count > 64) throw std::runtime_error("Invalid NVENC codec capability count");
        std::vector<GUID> codecs(codec_count);
        uint32_t written = 0;
        check(api.nvEncGetEncodeGUIDs(session, codecs.data(), codec_count, &written), "Read supported encode codecs");
        if (written > codec_count) throw std::runtime_error("Invalid NVENC codec capability result");
        const bool supported = std::any_of(codecs.begin(), codecs.begin() + written,
            [&codec](const GUID& candidate) { return IsEqualGUID(candidate, codec); });
        if (!supported)
            throw std::runtime_error(av1
                ? "AV1 hardware encoding is not supported by this GPU. Select HEVC/H.265 (Test-Streaming.ps1 -Role Publisher -Codec h265)."
                : "HEVC/H.265 hardware encoding is not supported by this GPU.");
        NV_ENC_CAPS_PARAM caps{};caps.version=NV_ENC_CAPS_PARAM_VER;
        int max_w=0,max_h=0;
        caps.capsToQuery=NV_ENC_CAPS_WIDTH_MAX;check(api.nvEncGetEncodeCaps(session,codec,&caps,&max_w),"Query NVENC maximum width");
        caps.capsToQuery=NV_ENC_CAPS_HEIGHT_MAX;check(api.nvEncGetEncodeCaps(session,codec,&caps,&max_h),"Query NVENC maximum height");
        if(max_w<2 || max_h<2)throw std::runtime_error("Invalid NVENC resolution capabilities");
        hc_fit_size(w,h,uint32_t(max_w),uint32_t(max_h));width=w;height=h;
        NV_ENC_PRESET_CONFIG preset{};
        preset.version = NV_ENC_PRESET_CONFIG_VER;
        preset.presetCfg.version = NV_ENC_CONFIG_VER;
        check(api.nvEncGetEncodePresetConfigEx(session, codec, NV_ENC_PRESET_P1_GUID,
            NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY, &preset), "Query low latency preset");
        config = preset.presetCfg;
        config.gopLength = fps;
        config.frameIntervalP = 1; // IPPP, bf=0
        config.rcParams.rateControlMode = NV_ENC_PARAMS_RC_CBR;
        config.rcParams.averageBitRate = bitrate;
        config.rcParams.maxBitRate = bitrate;
        config.rcParams.vbvBufferSize = bitrate / fps;
        config.rcParams.vbvInitialDelay = config.rcParams.vbvBufferSize;
        config.rcParams.enableLookahead = 0;
        config.rcParams.enableAQ = 0;
        config.rcParams.enableTemporalAQ = 0;
        config.rcParams.multiPass = NV_ENC_MULTI_PASS_DISABLED;
        if (av1) {
            config.encodeCodecConfig.av1Config.idrPeriod = fps;
            config.encodeCodecConfig.av1Config.repeatSeqHdr = 1;
        } else {
            config.profileGUID = NV_ENC_HEVC_PROFILE_MAIN_GUID;
            config.encodeCodecConfig.hevcConfig.idrPeriod = fps;
            config.encodeCodecConfig.hevcConfig.repeatSPSPPS = 1;
        }
        init.version = NV_ENC_INITIALIZE_PARAMS_VER;
        init.encodeGUID = codec; init.presetGUID = NV_ENC_PRESET_P1_GUID;
        init.encodeWidth = w; init.encodeHeight = h;
        init.darWidth = w; init.darHeight = h;
        init.frameRateNum = fps; init.frameRateDen = 1;
        init.enablePTD = 1; init.enableEncodeAsync = 0;
        init.tuningInfo = NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY;
        init.encodeConfig = &config;
        check(api.nvEncInitializeEncoder(session, &init), "Initialize NVENC");
        D3D11_TEXTURE2D_DESC texture{};
        texture.Width = w; texture.Height = h; texture.MipLevels = 1; texture.ArraySize = 1;
        texture.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
        texture.SampleDesc.Count = 1; texture.Usage = D3D11_USAGE_DEFAULT;
        texture.BindFlags = D3D11_BIND_RENDER_TARGET;
        hr(device->CreateTexture2D(&texture, nullptr, &input), "Allocate GPU input texture");
        D3D11_QUERY_DESC query{D3D11_QUERY_EVENT, 0};
        hr(device->CreateQuery(&query, &copy_done), "Create GPU copy fence");
        NV_ENC_REGISTER_RESOURCE resource{};
        resource.version = NV_ENC_REGISTER_RESOURCE_VER;
        resource.resourceType = NV_ENC_INPUT_RESOURCE_TYPE_DIRECTX;
        resource.resourceToRegister = input.Get();
        resource.width = w; resource.height = h;
        resource.bufferFormat = NV_ENC_BUFFER_FORMAT_ARGB;
        resource.bufferUsage = NV_ENC_INPUT_IMAGE;
        check(api.nvEncRegisterResource(session, &resource), "Register D3D11 texture");
        registered = resource.registeredResource;
        NV_ENC_CREATE_BITSTREAM_BUFFER buffer{};
        buffer.version = NV_ENC_CREATE_BITSTREAM_BUFFER_VER;
        check(api.nvEncCreateBitstreamBuffer(session, &buffer), "Create compressed output buffer");
        output = buffer.bitstreamBuffer;
    }

    void rates(uint32_t bitrate, uint32_t frame_rate) {
        if (bitrate < 64000 || bitrate > 45000000 || !frame_rate || frame_rate > fps)
            throw std::runtime_error("Invalid NVENC rate-control target");
        if(config.rcParams.averageBitRate==bitrate && init.frameRateNum==frame_rate)return;
        config.rcParams.averageBitRate = bitrate;
        config.rcParams.maxBitRate = bitrate;
        config.rcParams.vbvBufferSize = bitrate / frame_rate;
        config.rcParams.vbvInitialDelay = config.rcParams.vbvBufferSize;
        init.frameRateNum = frame_rate;
        NV_ENC_RECONFIGURE_PARAMS params{};
        params.version = NV_ENC_RECONFIGURE_PARAMS_VER;
        params.reInitEncodeParams = init;
        params.forceIDR = 0; // Rate-only updates must not reset the reference chain.
        check(api.nvEncReconfigureEncoder(session, &params), "Reconfigure rate control");
    }

    void encode(ID3D11Texture2D* source, uint64_t timestamp, bool keyframe,
                uint8_t* bytes, size_t capacity, size_t& length, bool& is_key) {
        if (!bytes || (!source && !input_ready)) throw std::runtime_error("No GPU frame to encode");
        if (source) {
        D3D11_TEXTURE2D_DESC desc{}; source->GetDesc(&desc);
        ComPtr<ID3D11Device> owner; source->GetDevice(&owner);
        if (owner.Get() != device.Get())
            throw std::runtime_error("Capture device changed; rebuild GPU pipeline");
        if (desc.Width != source_width || desc.Height != source_height)
            throw std::runtime_error("Capture dimensions changed: expected " + std::to_string(width) + "x" + std::to_string(height) +
                ", received " + std::to_string(desc.Width) + "x" + std::to_string(desc.Height) + "; rebuild GPU pipeline");
        if ((desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM && desc.Format != DXGI_FORMAT_R16G16B16A16_FLOAT) || desc.SampleDesc.Count != 1 || desc.ArraySize != 1 || desc.MipLevels != 1)
            throw std::runtime_error("Unsupported capture texture layout: format=" + std::to_string(desc.Format) +
                " samples=" + std::to_string(desc.SampleDesc.Count) + " array=" + std::to_string(desc.ArraySize) + " mips=" + std::to_string(desc.MipLevels));
        if (desc.Format == DXGI_FORMAT_R16G16B16A16_FLOAT || desc.Width!=width || desc.Height!=height)
            capture_color.convert(device.Get(), context.Get(), source, input.Get());
        else
            context->CopyResource(input.Get(), source);
        context->End(copy_done.Get()); context->Flush();
        const ULONGLONG start = GetTickCount64();
        while (true) {
            const HRESULT ready = context->GetData(copy_done.Get(), nullptr, 0, D3D11_ASYNC_GETDATA_DONOTFLUSH);
            hr(ready, "Wait GPU copy");
            if (ready == S_OK) break;
            if (GetTickCount64() - start > 1000) throw std::runtime_error("GPU copy timed out");
            Sleep(1);
        }
        input_ready = true;
        }
        NV_ENC_MAP_INPUT_RESOURCE map{};
        map.version = NV_ENC_MAP_INPUT_RESOURCE_VER; map.registeredResource = registered;
        check(api.nvEncMapInputResource(session, &map), "Map GPU input");
        mapped = map.mappedResource;
        NV_ENC_PIC_PARAMS picture{};
        picture.version = NV_ENC_PIC_PARAMS_VER;
        picture.inputBuffer = mapped; picture.bufferFmt = map.mappedBufferFmt;
        picture.inputWidth = width; picture.inputHeight = height;
        picture.outputBitstream = output; picture.pictureStruct = NV_ENC_PIC_STRUCT_FRAME;
        picture.inputTimeStamp = timestamp;
        if (keyframe) picture.encodePicFlags = NV_ENC_PIC_FLAG_FORCEIDR | NV_ENC_PIC_FLAG_OUTPUT_SPSPPS;
        check(api.nvEncEncodePicture(session, &picture), "Encode picture");
        NV_ENC_LOCK_BITSTREAM lock{};
        lock.version = NV_ENC_LOCK_BITSTREAM_VER; lock.outputBitstream = output;
        check(api.nvEncLockBitstream(session, &lock), "Wait compressed output");
        output_locked = true;
        if (!lock.bitstreamBufferPtr || lock.bitstreamSizeInBytes == 0 || lock.bitstreamSizeInBytes > capacity)
            throw std::runtime_error("Invalid compressed output buffer/size");
        length = lock.bitstreamSizeInBytes;
        std::memcpy(bytes, lock.bitstreamBufferPtr, length);
        is_key = lock.pictureType == NV_ENC_PIC_TYPE_IDR || lock.pictureType == NV_ENC_PIC_TYPE_I;
        check(api.nvEncUnlockBitstream(session, output), "Unlock compressed output");
        output_locked = false;
        check(api.nvEncUnmapInputResource(session, mapped), "Unmap GPU input");
        mapped = nullptr;
    }
};

extern "C" {
unsigned hc_nvenc_live_encoders() noexcept {return live_encoders.load();}
void hc_nvenc_size(void* handle,uint32_t* w,uint32_t* h) noexcept {if(handle && w && h){auto* e=static_cast<Encoder*>(handle);*w=e->width;*h=e->height;}}
void* hc_nvenc_open(void* device, uint32_t w, uint32_t h, uint32_t fps, uint32_t bitrate,
                    uint32_t av1, char* error, size_t capacity) noexcept {
    try {
        auto encoder = std::make_unique<Encoder>();
        encoder->open(static_cast<ID3D11Device*>(device), w, h, fps, bitrate, av1 != 0);
        return encoder.release();
    } catch (const std::exception& exception) { report(error, capacity, exception.what()); }
      catch (...) { report(error, capacity, "Unknown NVENC initialization error"); }
    return nullptr;
}
int hc_nvenc_encode(void* handle, void* texture, uint64_t timestamp, uint32_t keyframe,
                    uint8_t* bytes, size_t capacity, size_t* length, uint32_t* is_key,
                    char* error, size_t error_capacity) noexcept {
    try {
        if (!handle || !length || !is_key) throw std::runtime_error("Invalid encode arguments");
        bool key = false;
        static_cast<Encoder*>(handle)->encode(static_cast<ID3D11Texture2D*>(texture), timestamp,
            keyframe != 0, bytes, capacity, *length, key);
        *is_key = key; return 0;
    } catch (const std::exception& exception) { report(error, error_capacity, exception.what()); }
      catch (...) { report(error, error_capacity, "Unknown NVENC encode error"); }
    return -1;
}
int hc_nvenc_rates(void* handle, uint32_t bitrate, uint32_t fps, char* error, size_t capacity) noexcept {
    try {
        if (!handle) throw std::runtime_error("Invalid encoder handle");
        static_cast<Encoder*>(handle)->rates(bitrate, fps); return 0;
    } catch (const std::exception& exception) { report(error, capacity, exception.what()); }
      catch (...) { report(error, capacity, "Unknown rate-control error"); }
    return -1;
}
void hc_nvenc_close(void* handle) noexcept { delete static_cast<Encoder*>(handle); }
void* hc_nvenc_preview(void* handle) noexcept {
    try {
        auto encoder = static_cast<Encoder*>(handle);
        if (!encoder || !encoder->input_ready) return nullptr;
        // Immutable snapshot: UI must never read the texture NVENC overwrites.
        ComPtr<ID3D10Multithread> protect;
        Encoder::hr(encoder->context.As(&protect), "Preview context protection");
        protect->SetMultithreadProtected(TRUE);
        D3D11_TEXTURE2D_DESC desc{}; encoder->input->GetDesc(&desc);
        desc.BindFlags = D3D11_BIND_RENDER_TARGET;
        ComPtr<ID3D11Texture2D> snapshot;
        Encoder::hr(encoder->device->CreateTexture2D(&desc, nullptr, &snapshot), "Preview GPU snapshot");
        encoder->context->CopyResource(snapshot.Get(), encoder->input.Get());
        encoder->context->End(encoder->copy_done.Get()); encoder->context->Flush();
        auto start = GetTickCount64();
        for (;;) {
            auto status = encoder->context->GetData(encoder->copy_done.Get(), nullptr, 0, D3D11_ASYNC_GETDATA_DONOTFLUSH);
            Encoder::hr(status, "Wait preview copy");
            if (status == S_OK) break;
            if (GetTickCount64() - start > 1000) return nullptr;
            Sleep(0);
        }
        return snapshot.Detach();
    } catch (...) { return nullptr; }
}
}
