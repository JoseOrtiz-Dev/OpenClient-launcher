#pragma once
#include <QFileInfo>
#include <QString>

// Accesos con el nombre de la instancia (junctions NTFS) hacia las carpetas reales `instances/<uuid>`.
namespace InstanceLinks {
// Nombre de carpeta válido en Windows a partir del nombre de la instancia (máx. 60 caracteres).
QString sanitize(const QString &name);
// Crea una junction `link` -> `target` (ruta absoluta de un directorio existente). No requiere administrador.
bool createJunction(const QString &link, const QString &target);
// Destino de una junction o enlace simbólico (symLinkTarget() no resuelve las junctions).
QString target(const QFileInfo &link);
}
