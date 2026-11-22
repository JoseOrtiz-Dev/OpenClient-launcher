#pragma once

#include <QImage>
#include <QQuickPaintedItem>
#include <QtQml/qqmlregistration.h>

class SkinPreview : public QQuickPaintedItem {
    Q_OBJECT
    QML_ELEMENT
    Q_PROPERTY(QImage skin READ skin WRITE setSkin)
    Q_PROPERTY(QImage cape READ cape WRITE setCape)
    Q_PROPERTY(bool slim READ slim WRITE setSlim)
    Q_PROPERTY(bool capeVisible READ capeVisible WRITE setCapeVisible)

public:
    explicit SkinPreview(QQuickItem *parent = nullptr);
    QImage skin() const { return m_skin; }
    QImage cape() const { return m_cape; }
    bool slim() const { return m_slim; }
    bool capeVisible() const { return m_capeVisible; }
    void setSkin(const QImage &image);
    void setCape(const QImage &image);
    void setSlim(bool value);
    void setCapeVisible(bool value);
    void paint(QPainter *painter) override;
    Q_INVOKABLE void orbit(qreal dx, qreal dy);
    Q_INVOKABLE void zoomBy(qreal steps);
    Q_INVOKABLE void setView(bool back);

private:
    static constexpr qreal frontYaw = -32;
    static constexpr qreal backYaw = 148;
    static constexpr qreal defaultPitch = -21;
    QImage m_skin;
    QImage m_cape;
    bool m_slim = false;
    bool m_capeVisible = true;
    qreal m_yaw = frontYaw;
    qreal m_pitch = defaultPitch;
    qreal m_zoom = 1;
};
