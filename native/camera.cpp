#include <windows.h>
#include <mfapi.h>
#include <mfidl.h>
#include <mfreadwrite.h>
#include <wrl/client.h>
#include <cstdint>
#include <cstring>
#include <memory>
#include <new>

using Microsoft::WRL::ComPtr;

namespace {
constexpr UINT32 width = 1280;
constexpr UINT32 height = 720;

struct Camera {
    bool com_initialized = false;
    bool mf_initialized = false;
    ComPtr<IMFMediaSource> source;
    ComPtr<IMFSourceReader> reader;

    ~Camera() {
        reader.Reset();
        if (source) source->Shutdown();
        source.Reset();
        if (mf_initialized) MFShutdown();
        if (com_initialized) CoUninitialize();
    }
};

HRESULT open_camera(Camera& camera) {
    HRESULT hr = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    if (FAILED(hr)) return hr;
    camera.com_initialized = true;
    hr = MFStartup(MF_VERSION);
    if (FAILED(hr)) return hr;
    camera.mf_initialized = true;

    ComPtr<IMFAttributes> attributes;
    if (FAILED(hr = MFCreateAttributes(&attributes, 1))) return hr;
    if (FAILED(hr = attributes->SetGUID(MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                                       MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID))) return hr;
    IMFActivate** devices = nullptr;
    UINT32 count = 0;
    if (FAILED(hr = MFEnumDeviceSources(attributes.Get(), &devices, &count))) return hr;
    hr = count ? devices[0]->ActivateObject(IID_PPV_ARGS(&camera.source)) : HRESULT_FROM_WIN32(ERROR_NOT_FOUND);
    for (UINT32 i = 0; i < count; ++i) devices[i]->Release();
    CoTaskMemFree(devices);
    if (FAILED(hr)) return hr;

    if (FAILED(hr = MFCreateAttributes(&attributes, 1))) return hr;
    if (FAILED(hr = attributes->SetUINT32(MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, TRUE))) return hr;
    if (FAILED(hr = MFCreateSourceReaderFromMediaSource(camera.source.Get(), attributes.Get(), &camera.reader))) return hr;
    ComPtr<IMFMediaType> format;
    if (FAILED(hr = MFCreateMediaType(&format))) return hr;
    if (FAILED(hr = format->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video))) return hr;
    if (FAILED(hr = format->SetGUID(MF_MT_SUBTYPE, MFVideoFormat_NV12))) return hr;
    if (FAILED(hr = MFSetAttributeSize(format.Get(), MF_MT_FRAME_SIZE, width, height))) return hr;
    if (FAILED(hr = MFSetAttributeRatio(format.Get(), MF_MT_FRAME_RATE, 30, 1))) return hr;
    return camera.reader->SetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM, nullptr, format.Get());
}
}

extern "C" void* hc_camera_open(int32_t* error) {
    auto camera = std::unique_ptr<Camera>(new (std::nothrow) Camera());
    if (!camera) {
        *error = E_OUTOFMEMORY;
        return nullptr;
    }
    *error = open_camera(*camera);
    return FAILED(*error) ? nullptr : camera.release();
}

extern "C" int32_t hc_camera_read(void* handle, uint8_t* nv12, uint32_t size) {
    auto& camera = *static_cast<Camera*>(handle);
    if (size != width * height * 3 / 2) return E_INVALIDARG;
    DWORD flags = 0;
    ComPtr<IMFSample> sample;
    HRESULT hr = camera.reader->ReadSample(MF_SOURCE_READER_FIRST_VIDEO_STREAM, 0,
                                           nullptr, &flags, nullptr, &sample);
    if (FAILED(hr)) return hr;
    if (flags & (MF_SOURCE_READERF_ERROR | MF_SOURCE_READERF_ENDOFSTREAM |
                 MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED)) return E_FAIL;
    if (!sample) return S_FALSE;

    ComPtr<IMFMediaBuffer> buffer;
    if (FAILED(hr = sample->ConvertToContiguousBuffer(&buffer))) return hr;
    ComPtr<IMF2DBuffer> image;
    if (SUCCEEDED(buffer.As(&image))) {
        DWORD length = 0;
        if (FAILED(hr = image->GetContiguousLength(&length))) return hr;
        if (length != size) return E_FAIL;
        return image->ContiguousCopyTo(nv12, size);
    }
    BYTE* data = nullptr;
    DWORD length = 0;
    if (FAILED(hr = buffer->Lock(&data, nullptr, &length))) return hr;
    if (length == size) std::memcpy(nv12, data, size);
    buffer->Unlock();
    return length == size ? S_OK : E_FAIL;
}

extern "C" void hc_camera_close(void* handle) {
    delete static_cast<Camera*>(handle);
}
