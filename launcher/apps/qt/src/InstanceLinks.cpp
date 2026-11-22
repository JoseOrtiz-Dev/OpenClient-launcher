#include "InstanceLinks.h"
#include <QDir>
#include <QProcess>
#include <QRegularExpression>
#include <QFile>
#include <cstring>
#ifdef Q_OS_WIN
#ifndef NOMINMAX
#define NOMINMAX
#endif
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <winioctl.h>
#endif

namespace InstanceLinks {

QString sanitize(const QString &name) {
    static const QRegularExpression invalid(QStringLiteral("[<>:\"/\\|?*\\x{0000}-\\x{001F}\\x{007F}]"));
    static const QRegularExpression reserved(QStringLiteral("^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$"), QRegularExpression::CaseInsensitiveOption);
    auto result = QString(name).remove(invalid).trimmed();
    if (result.size() > 60) {
        result.truncate(60);
        if (result.back().isHighSurrogate()) result.chop(1);
    }
    while (result.endsWith('.') || result.endsWith(' ')) result.chop(1);
    // Windows también reserva «CON.txt»: el guion bajo va justo tras el nombre base.
    const auto dot = result.indexOf('.');
    const auto base = dot < 0 ? result.size() : dot;
    if (reserved.match(result.left(base).trimmed()).hasMatch()) result.insert(base, '_');
    return result.isEmpty() ? QStringLiteral("Instancia") : result;
}

#ifdef Q_OS_WIN
namespace {
bool setMountPoint(const QString &link, const QString &target) {
    const auto native = QDir::toNativeSeparators(target);
    const auto substitute = QStringLiteral("\\??\\") + native;   // \??\C:\…
    if (substitute.size() > 1000 || native.startsWith(QStringLiteral("\\\\"))) return false;
    const auto subBytes = quint16(substitute.size() * 2);
    const auto printBytes = quint16(native.size() * 2);
    // REPARSE_DATA_BUFFER (mount point): cabecera de 8 bytes + 4 longitudes/desplazamientos + ruta de sustitución y de impresión.
    QByteArray buffer(16 + subBytes + 2 + printBytes + 2, 0);
    const auto put = [&buffer](int offset, const void *value, int size) { std::memcpy(buffer.data() + offset, value, size); };
    const quint32 tag = IO_REPARSE_TAG_MOUNT_POINT;
    const quint16 dataLength = quint16(buffer.size() - 8), substituteOffset = 0, substituteLength = subBytes,
                  printOffset = quint16(subBytes + 2), printLength = printBytes;
    put(0, &tag, 4);
    put(4, &dataLength, 2);
    put(8, &substituteOffset, 2);
    put(10, &substituteLength, 2);
    put(12, &printOffset, 2);
    put(14, &printLength, 2);
    put(16, substitute.utf16(), subBytes);
    put(16 + subBytes + 2, native.utf16(), printBytes);
    const auto path = QDir::toNativeSeparators(link);
    const auto wide = reinterpret_cast<LPCWSTR>(path.utf16());
    if (!CreateDirectoryW(wide, nullptr)) return false;
    HANDLE handle = CreateFileW(wide, GENERIC_WRITE, 0, nullptr, OPEN_EXISTING,
                                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT, nullptr);
    DWORD returned = 0;
    const bool ok = handle != INVALID_HANDLE_VALUE &&
        DeviceIoControl(handle, FSCTL_SET_REPARSE_POINT, buffer.data(), DWORD(buffer.size()), nullptr, 0, &returned, nullptr);
    if (handle != INVALID_HANDLE_VALUE) CloseHandle(handle);
    if (!ok) RemoveDirectoryW(wide);
    return ok;
}
}
#endif

QString target(const QFileInfo &link) {
    auto path = QDir::fromNativeSeparators(link.isJunction() ? link.junctionTarget() : link.symLinkTarget());
    // Una junction guarda su destino como `\??\C:\…`. Si Qt deja el prefijo, se quita; un destino mal formado
    // (`??\` sin la primera barra, de una versión anterior) queda distinto del esperado y se repara al sincronizar.
    if (path.startsWith(QStringLiteral("/??/"))) path = path.mid(4);
    return path;
}

bool createJunction(const QString &link, const QString &target) {
#ifdef Q_OS_WIN
    if (setMountPoint(link, target)) return true;
    // Alternativa: mklink /J. Los nombres ya vienen saneados y los argumentos no pasan por una shell propia.
    QProcess process;
    process.start(QStringLiteral("cmd.exe"), {QStringLiteral("/c"), QStringLiteral("mklink"), QStringLiteral("/J"),
                                              QDir::toNativeSeparators(link), QDir::toNativeSeparators(target)});
    if (!process.waitForFinished(5000)) { process.kill(); return false; }
    return process.exitStatus() == QProcess::NormalExit && process.exitCode() == 0;
#else
    return QFile::link(target, link);
#endif
}

}
