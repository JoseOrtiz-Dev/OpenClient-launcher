#include "WindowFrame.h"
#include <QWindow>

#ifdef Q_OS_WIN
#define NOMINMAX
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <dwmapi.h>

namespace {
// Valores de DWMWINDOWATTRIBUTE; se definen aquí porque los SDK antiguos no los incluyen.
constexpr DWORD DarkMode = 20;
constexpr DWORD BorderColor = 34;
constexpr DWORD CaptionColor = 35;
constexpr DWORD TextColor = 36;

COLORREF colorRef(const QColor &color) {
    return RGB(color.red(), color.green(), color.blue());
}
}

void styleWindowFrame(QWindow *window, const QColor &caption, const QColor &text, const QColor &border, bool dark) {
    if (!window) return;
    const auto handle = reinterpret_cast<HWND>(window->winId());
    const BOOL useDark = dark ? TRUE : FALSE;
    DwmSetWindowAttribute(handle, DarkMode, &useDark, sizeof(useDark));
    const COLORREF captionRef = colorRef(caption), textRef = colorRef(text), borderRef = colorRef(border);
    DwmSetWindowAttribute(handle, CaptionColor, &captionRef, sizeof(captionRef));
    DwmSetWindowAttribute(handle, TextColor, &textRef, sizeof(textRef));
    DwmSetWindowAttribute(handle, BorderColor, &borderRef, sizeof(borderRef));
}
#else
void styleWindowFrame(QWindow *, const QColor &, const QColor &, const QColor &, bool) {}
#endif
