// Source-only host seam. NOT linked into the application or qualified for launch.
// Build against official Microsoft.Web.WebView2 1.0.4314.35 headers/loader.
// Host owns its HWND/message loop and cancels/joins workers before Close().
#include <windows.h>
#include <WebView2.h>
#include <wrl.h>
#include <string>

namespace review_shell {
using Microsoft::WRL::ComPtr;
// The host creates the environment with a dedicated data directory, no browser
// flags or profile sharing. This seam never creates a worker or approves a job.
class WindowBinding {
    HWND window_ = nullptr;
    ComPtr<ICoreWebView2Controller> controller_;
    ComPtr<ICoreWebView2> view_;
public:
    HRESULT Attach(HWND window, ICoreWebView2Controller* controller, unsigned short port) {
        if (!window || !IsWindow(window) || !controller || !port || controller_) return E_INVALIDARG;
        ComPtr<ICoreWebView2> view;
        HRESULT hr=controller->get_CoreWebView2(&view);
        if (FAILED(hr)) return hr;
        RECT bounds{};
        if (!GetClientRect(window,&bounds)) return HRESULT_FROM_WIN32(GetLastError());
        hr=controller->put_Bounds(bounds);
        if (FAILED(hr)) return hr;
        // Exact loopback Controller only; no custom transport/security fallback.
        const auto origin=L"http://127.0.0.1:"+std::to_wstring(port)+L"/controller";
        hr=view->Navigate(origin.c_str());
        if (FAILED(hr)) return hr;
        window_=window;controller_=controller;view_=view;
        return S_OK; // navigation requested, NOT UI-ready or render authority
    }
    HRESULT Resize() {
        if (!controller_) return E_UNEXPECTED;
        RECT bounds{};
        if (!GetClientRect(window_,&bounds)) return HRESULT_FROM_WIN32(GetLastError());
        return controller_->put_Bounds(bounds);
    }
    HRESULT CloseAfterWorkerJoin() {
        if (!controller_) return S_FALSE;
        const auto hr=controller_->Close();
        if (SUCCEEDED(hr)) {view_.Reset();controller_.Reset();window_=nullptr;}
        return hr;
    }
};
} // namespace review_shell
