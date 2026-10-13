// WASAPI process loopback excludes HyperCast and its children: no microphone,
// no recapture of received room audio. PCM is 48 kHz stereo, 16 bit.
#define NOMINMAX
#include <windows.h>
#include <audioclient.h>
#include <audioclientactivationparams.h>
#include <mmdeviceapi.h>
#include <wrl.h>
#include <algorithm>
#include <cstring>
#include <memory>
#include <stdexcept>
#include <string>
using Microsoft::WRL::ComPtr;

static void check(HRESULT hr, const char* operation) {
    if (FAILED(hr)) throw std::runtime_error(std::string(operation) + ": HRESULT " + std::to_string(hr));
}
static void report(char* error, size_t size, const char* text) noexcept {
    if (error && size) { size_t n = std::min(size - 1, std::strlen(text)); std::memcpy(error, text, n); error[n] = 0; }
}
struct ComScope {
    ComScope() { check(CoInitializeEx(nullptr, COINIT_MULTITHREADED), "Initialize audio COM"); }
    ~ComScope() { CoUninitialize(); }
};
struct Event {
    HANDLE value = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    Event() { if (!value) throw std::runtime_error("Create audio event failed"); }
    ~Event() { CloseHandle(value); }
};
static WAVEFORMATEX pcm() {
    WAVEFORMATEX format{};
    format.wFormatTag = WAVE_FORMAT_PCM; format.nChannels = 2; format.nSamplesPerSec = 48000;
    format.wBitsPerSample = 16; format.nBlockAlign = 4; format.nAvgBytesPerSec = 192000;
    return format;
}
class Completion final : public Microsoft::WRL::RuntimeClass<
    Microsoft::WRL::RuntimeClassFlags<Microsoft::WRL::ClassicCom>,
    IActivateAudioInterfaceCompletionHandler, Microsoft::WRL::FtmBase> {
public:
    Event done;
    AUDIOCLIENT_ACTIVATION_PARAMS params{};
    ComPtr<IAudioClient> client;
    HRESULT result = E_PENDING;
    STDMETHOD(ActivateCompleted)(IActivateAudioInterfaceAsyncOperation* operation) override {
        ComPtr<IUnknown> unknown;
        HRESULT activation = E_FAIL;
        result = operation->GetActivateResult(&activation, &unknown);
        if (SUCCEEDED(result)) result = activation;
        if (SUCCEEDED(result)) result = unknown.As(&client);
        SetEvent(done.value);
        return S_OK;
    }
};
struct Capture {
    ComScope com;
    ComPtr<IAudioClient> client;
    ComPtr<IAudioCaptureClient> capture;
    Event ready;
    bool started = false;
    Capture(DWORD process = 0) {
        if (process == GetCurrentProcessId()) throw std::runtime_error("Cannot capture HyperCast audio");
        auto completion = Microsoft::WRL::Make<Completion>();
        completion->params.ActivationType = AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK;
        completion->params.ProcessLoopbackParams.TargetProcessId = process ? process : GetCurrentProcessId();
        completion->params.ProcessLoopbackParams.ProcessLoopbackMode = process ? PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE : PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE;
        PROPVARIANT properties{}; properties.vt = VT_BLOB;
        properties.blob.cbSize = sizeof(completion->params);
        properties.blob.pBlobData = reinterpret_cast<BYTE*>(&completion->params);
        ComPtr<IActivateAudioInterfaceAsyncOperation> operation;
        check(ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, __uuidof(IAudioClient),
            &properties, completion.Get(), &operation), "System audio requires Windows build 20348+ (Windows 11)");
        if (WaitForSingleObject(completion->done.value, 5000) != WAIT_OBJECT_0)
            throw std::runtime_error("System audio activation timed out");
        check(completion->result, "Activate system audio; Windows build 20348+ required");
        client = completion->client;
        auto format = pcm();
        check(client->Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK |
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
            0, 0, &format, nullptr), "Initialize system audio capture");
        check(client->GetService(IID_PPV_ARGS(&capture)), "Open system audio capture");
        check(client->SetEventHandle(ready.value), "Set audio capture event");
        check(client->Start(), "Start system audio capture"); started = true;
    }
    ~Capture() { if (started) client->Stop(); }
    uint32_t read(int16_t* samples, uint32_t capacity) {
        UINT32 size = 0;
        check(capture->GetNextPacketSize(&size), "Poll system audio");
        if (!size) {
            auto wait = WaitForSingleObject(ready.value, 10);
            if (wait == WAIT_FAILED) throw std::runtime_error("Audio capture event failed");
            check(capture->GetNextPacketSize(&size), "Poll system audio");
            if (!size) return 0;
        }
        BYTE* data = nullptr; DWORD flags = 0; UINT32 frames = 0;
        check(capture->GetBuffer(&data, &frames, &flags, nullptr, nullptr), "Get system audio packet");
        if (frames > capacity / 2) { capture->ReleaseBuffer(frames); throw std::runtime_error("System audio packet exceeds bound"); }
        if (flags & AUDCLNT_BUFFERFLAGS_SILENT) std::memset(samples, 0, frames * 4);
        else if (data) std::memcpy(samples, data, frames * 4);
        else { capture->ReleaseBuffer(frames); throw std::runtime_error("Invalid system audio packet"); }
        check(capture->ReleaseBuffer(frames), "Release system audio packet");
        return frames;
    }
};
struct Playout {
    ComScope com;
    ComPtr<IAudioClient> client;
    ComPtr<IAudioRenderClient> render;
    UINT32 capacity = 0;
    bool started = false;
    Playout() {
        ComPtr<IMMDeviceEnumerator> enumerator; ComPtr<IMMDevice> endpoint;
        check(CoCreateInstance(__uuidof(MMDeviceEnumerator), nullptr, CLSCTX_ALL, IID_PPV_ARGS(&enumerator)), "Enumerate audio output");
        check(enumerator->GetDefaultAudioEndpoint(eRender, eMultimedia, &endpoint), "Default audio output unavailable");
        check(endpoint->Activate(__uuidof(IAudioClient), CLSCTX_ALL, nullptr, reinterpret_cast<void**>(client.GetAddressOf())), "Activate audio output");
        auto format = pcm();
        check(client->Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM |
            AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, 200000, 0, &format, nullptr), "Initialize audio output");
        check(client->GetBufferSize(&capacity), "Audio output capacity");
        check(client->GetService(IID_PPV_ARGS(&render)), "Open audio renderer");
        check(client->Start(), "Start audio output"); started = true;
    }
    ~Playout() { if (started) client->Stop(); }
    bool write(const int16_t* samples, uint32_t frames) {
        if (!frames || frames > 960 || frames > capacity) throw std::runtime_error("Invalid audio output frame size");
        UINT32 padding = 0; check(client->GetCurrentPadding(&padding), "Audio output padding");
        // Bound OS queue too, even when the endpoint allocates a large buffer.
        if (padding > 960 || padding > capacity || frames > capacity - padding) return false;
        BYTE* data = nullptr; check(render->GetBuffer(frames, &data), "Get audio output buffer");
        std::memcpy(data, samples, frames * 4);
        check(render->ReleaseBuffer(frames, 0), "Release audio output buffer");
        return true;
    }
};
extern "C" {
void* hc_audio_capture_open(char* error, size_t size) noexcept {
    try { return new Capture(); } catch (const std::exception& e) { report(error,size,e.what()); } catch (...) { report(error,size,"Audio capture initialization failed"); } return nullptr;
}
void* hc_audio_capture_open_process(unsigned process, char* error, size_t size) noexcept {
    try { return new Capture(process); } catch (const std::exception& e) { report(error,size,e.what()); } catch (...) { report(error,size,"Application audio capture failed"); } return nullptr;
}
int hc_audio_capture_read(void* handle, int16_t* samples, uint32_t capacity, char* error, size_t size) noexcept {
    try { if (!handle || !samples || capacity > 96000) throw std::runtime_error("Invalid capture arguments"); return static_cast<Capture*>(handle)->read(samples,capacity); }
    catch (const std::exception& e) { report(error,size,e.what()); } catch (...) { report(error,size,"Audio capture failed"); } return -1;
}
void hc_audio_capture_close(void* handle) noexcept { delete static_cast<Capture*>(handle); }
void* hc_audio_playout_open(char* error, size_t size) noexcept {
    try { return new Playout(); } catch (const std::exception& e) { report(error,size,e.what()); } catch (...) { report(error,size,"Audio playback initialization failed"); } return nullptr;
}
int hc_audio_playout_write(void* handle, const int16_t* samples, uint32_t frames, char* error, size_t size) noexcept {
    try { if (!handle || !samples) throw std::runtime_error("Invalid playback arguments"); return static_cast<Playout*>(handle)->write(samples,frames) ? 1 : 0; }
    catch (const std::exception& e) { report(error,size,e.what()); } catch (...) { report(error,size,"Audio playback failed"); } return -1;
}
void hc_audio_playout_close(void* handle) noexcept { delete static_cast<Playout*>(handle); }
}
