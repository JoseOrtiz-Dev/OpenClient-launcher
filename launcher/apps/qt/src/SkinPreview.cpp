#include "SkinPreview.h"

#include <QMatrix4x4>
#include <QPainter>
#include <QPolygonF>
#include <QVector3D>
#include <algorithm>
#include <array>
#include <cmath>
#include <numbers>
#include <vector>

namespace {
enum Side { Front, Back, Left, Right, Top, Bottom };

struct Face {
    QPolygonF quad;
    qreal depth;
    const QImage *image;
    QRect source;
    QColor base;
    int shade;
};

QRect atlasFace(QPoint origin, int w, int h, int d, Side side) {
    const int x = origin.x();
    const int y = origin.y();
    switch (side) {
    case Front: return {x + d, y + d, w, h};
    case Back: return {x + w + 2 * d, y + d, w, h};
    case Left: return {x, y + d, d, h};
    case Right: return {x + w + d, y + d, d, h};
    case Top: return {x + d, y, w, d};
    case Bottom: return {x + w + d, y, w, d};
    }
    return {};
}

QRect scaledRect(const QRect &rect, int scale) {
    return {rect.x() * scale, rect.y() * scale, rect.width() * scale, rect.height() * scale};
}
}

SkinPreview::SkinPreview(QQuickItem *parent) : QQuickPaintedItem(parent) {}

void SkinPreview::setSkin(const QImage &image) { m_skin = image; update(); }
void SkinPreview::setCape(const QImage &image) { m_cape = image; update(); }
void SkinPreview::setSlim(bool value) { m_slim = value; update(); }
void SkinPreview::setCapeVisible(bool value) { m_capeVisible = value; update(); }

void SkinPreview::orbit(qreal dx, qreal dy) {
    m_yaw = std::remainder(m_yaw + dx * 0.6, 360.0);
    m_pitch = std::clamp(m_pitch - dy * 0.6, -80.0, 80.0);
    update();
}

void SkinPreview::zoomBy(qreal steps) {
    m_zoom = std::clamp(m_zoom * std::pow(1.12, steps), 0.65, 1.7);
    update();
}

void SkinPreview::setView(bool back) {
    m_yaw = back ? backYaw : frontYaw;
    m_pitch = defaultPitch;
    m_zoom = 1;
    update();
}

void SkinPreview::paint(QPainter *painter) {
    if (width() <= 0 || height() <= 0) return;
    QMatrix4x4 view;
    view.rotate(float(m_pitch), 1, 0, 0);
    view.rotate(float(m_yaw), 0, 1, 0);
    const auto scale = std::min(width() / 24.0, height() / 39.0) * m_zoom;
    const QPointF center(width() / 2, height() / 2);
    constexpr float viewDistance = 62;
    const QVector3D eye(0, 0, viewDistance);
    const QVector3D light = QVector3D(-0.45f, -0.62f, 0.64f).normalized();
    const auto project = [&](const QVector3D &point) {
        return center + QPointF(point.x(), point.y()) * (scale * viewDistance / (viewDistance - point.z()));
    };
    std::vector<Face> faces;
    faces.reserve(42);
    QMatrix4x4 pose;

    const auto box = [&](const QVector3D &origin, int w, int h, int depthPixels, const QImage &image,
                         QPoint uv, const QColor &base, float inflate, bool mirrored) {
        const float x = origin.x() - inflate, r = origin.x() + w + inflate;
        const float y = origin.y() - inflate, b = origin.y() + h + inflate;
        const float z = origin.z() - inflate, f = origin.z() + depthPixels + inflate;
        std::array<QVector3D, 8> corners = {
            QVector3D(x,y,z), QVector3D(r,y,z), QVector3D(r,b,z), QVector3D(x,b,z),
            QVector3D(x,y,f), QVector3D(r,y,f), QVector3D(r,b,f), QVector3D(x,b,f)
        };
        const auto transform = view * pose;
        for (auto &point : corners) point = transform.map(point);
        const std::array<std::array<int, 4>, 6> quads = {{
            {4,5,6,7}, {1,0,3,2}, {0,4,7,3}, {5,1,2,6}, {0,1,5,4}, {7,6,2,3}
        }};
        const int ratio = std::max(1, image.width() / 64);
        for (int i = 0; i < int(quads.size()); ++i) {
            const auto &indices = quads[i];
            const auto &first = corners[indices[0]];
            const auto normal = QVector3D::normal(corners[indices[1]] - first, corners[indices[2]] - first);
            if (QVector3D::dotProduct(normal, eye - first) <= 0) continue;
            QPolygonF quad;
            qreal depth = 0;
            for (const int index : indices) {
                const auto &point = corners[index];
                quad.append(project(point));
                depth += point.z();
            }
            const auto side = Side(mirrored && i == Left ? Right : mirrored && i == Right ? Left : i);
            if (mirrored) quad = QPolygonF{quad[1], quad[0], quad[3], quad[2]};
            const int shade = qRound(96 * (1 - std::max(0.0f, QVector3D::dotProduct(light, normal))));
            faces.push_back({quad, depth, &image, scaledRect(atlasFace(uv, w, h, depthPixels, side), ratio), base, shade});
        }
    };

    const bool legacy = m_skin.size() == QSize(64, 32);
    const int arm = m_slim && !legacy ? 3 : 4;
    const QPoint none(-1, -1);
    const QColor faceColor("#b79c8a"), topColor("#556775"), pantsColor("#343d50");
    const auto part = [&](const QVector3D &origin, int w, int h, int depthPixels, QPoint uv, QPoint overlay,
                          const QColor &base, bool mirrored = false) {
        box(origin, w, h, depthPixels, m_skin, uv, base, 0, mirrored);
        if (!m_skin.isNull() && overlay.x() >= 0)
            box(origin, w, h, depthPixels, m_skin, overlay, {}, h == 8 ? 0.5f : 0.25f, mirrored);
    };
    part({-4, -16, -4}, 8, 8, 8, {0,0}, {32,0}, faceColor);
    part({-4, -8, -2}, 8, 12, 4, {16,16}, legacy ? none : QPoint(16,32), topColor);
    part({-4.4f - arm, -8, -2}, arm, 12, 4, {40,16}, legacy ? none : QPoint(40,32), faceColor);
    part({4.4f, -8, -2}, arm, 12, 4, legacy ? QPoint(40,16) : QPoint(32,48),
        legacy ? none : QPoint(48,48), faceColor, legacy);
    part({-4.15f, 4, -2}, 4, 12, 4, {0,16}, legacy ? none : QPoint(0,32), pantsColor);
    part({0.15f, 4, -2}, 4, 12, 4, legacy ? QPoint(0,16) : QPoint(16,48),
        legacy ? none : QPoint(0,48), pantsColor, legacy);
    if (m_capeVisible && !m_cape.isNull()) {
        pose.translate(0, -8, -2.6f);
        pose.rotate(-11, 1, 0, 0);
        // The cape atlas puts its exterior on the front of its own cuboid.
        pose.rotate(180, 0, 1, 0);
        box({-5, 0, 0}, 10, 16, 1, m_cape, {0,0}, {}, 0, false);
    }

    QPolygonF shadow;
    constexpr int shadowSegments = 32;
    for (int i = 0; i < shadowSegments; ++i) {
        const auto angle = i * (2 * std::numbers::pi_v<float> / shadowSegments);
        shadow.append(project(view.map(QVector3D(9 * std::cos(angle), 16.55f, 4 * std::sin(angle)))));
    }
    painter->save();
    painter->setRenderHint(QPainter::Antialiasing);
    painter->setPen(Qt::NoPen);
    painter->setBrush(QColor(0, 0, 0, 44));
    painter->drawPolygon(shadow);
    painter->restore();

    std::stable_sort(faces.begin(), faces.end(), [](const Face &a, const Face &b) { return a.depth < b.depth; });
    const QPolygonF unit{{0,0},{1,0},{1,1},{0,1}};
    painter->setRenderHint(QPainter::SmoothPixmapTransform, false);
    for (const auto &face : faces) {
        QTransform textureTransform;
        if (!QTransform::quadToQuad(unit, face.quad, textureTransform)) continue;
        painter->save();
        painter->setWorldTransform(textureTransform, true);
        const QRectF tile(0, 0, 1, 1);
        if (face.base.isValid()) {
            painter->fillRect(tile, face.image->isNull() ? face.base : QColor(Qt::black));
            if (!face.image->isNull()) painter->drawImage(tile, *face.image, face.source);
            painter->fillRect(tile, QColor(0, 0, 0, face.shade));
        } else {
            auto texture = face.image->copy(face.source).convertToFormat(QImage::Format_ARGB32_Premultiplied);
            {
                QPainter lighting(&texture);
                lighting.setCompositionMode(QPainter::CompositionMode_SourceAtop);
                lighting.fillRect(texture.rect(), QColor(0, 0, 0, face.shade));
            }
            painter->drawImage(tile, texture);
        }
        painter->restore();
    }
}
