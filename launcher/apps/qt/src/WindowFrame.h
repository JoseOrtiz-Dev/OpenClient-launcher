#pragma once

#include <QColor>

class QWindow;

// Colorea la barra de título y el borde nativos de Windows con los colores del tema.
// En versiones de Windows sin soporte, los atributos se ignoran y queda el marco estándar.
void styleWindowFrame(QWindow *window, const QColor &caption, const QColor &text, const QColor &border, bool dark);
