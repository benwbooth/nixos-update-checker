#ifndef TERMINAL_WIDGET_H
#define TERMINAL_WIDGET_H

#include <QObject>
#include <QPointer>
#include <QQuickPaintedItem>
#include <QTimer>
#include <QWidget>
#include <qtermwidget6/qtermwidget.h>

// The update's terminal, drawn inside the QML scene.
//
// A QTermWidget runs off screen (WA_DontShowOnScreen: shown for Qt's
// purposes, no window of its own) and this item paints it with
// QWidget::render and forwards mouse, wheel and keyboard input to it. It used
// to be a native child window in a WindowContainer, but on Wayland that is a
// subsurface the compositor maps, moves and unmaps apart from the window's
// own frames, so showing or hiding it flickered. As a scene item it changes
// in the same frame as everything else.
class TerminalWidget : public QQuickPaintedItem {
    Q_OBJECT
    Q_PROPERTY(bool running READ running WRITE setRunning NOTIFY runningChanged)

public:
    explicit TerminalWidget(QQuickItem *parent = nullptr);
    ~TerminalWidget();

    void paint(QPainter *painter) override;

    bool running() const { return m_running; }
    void setRunning(bool running);

    Q_INVOKABLE void runCommand(const QString &command);
    Q_INVOKABLE QString getAllText();
    Q_INVOKABLE void clear();
    Q_INVOKABLE void focusTerminal();
    Q_INVOKABLE void log(const QString &msg);
    Q_INVOKABLE bool hasContent() const;
    Q_INVOKABLE void copySelection();
    Q_INVOKABLE void paste();
    Q_INVOKABLE void selectAll();

signals:
    void runningChanged();

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;
    void geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void mouseDoubleClickEvent(QMouseEvent *event) override;
    void wheelEvent(QWheelEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;
    void keyReleaseEvent(QKeyEvent *event) override;
    void focusInEvent(QFocusEvent *event) override;
    void focusOutEvent(QFocusEvent *event) override;
    void itemChange(ItemChange change, const ItemChangeData &value) override;

private:
    void setupTerminal();
    void showContextMenu(const QPoint &pos);
    bool handleDragScroll(QWidget *display, QEvent *event);
    void dragScrollTick();
    bool swallowWheel() const;
    void forwardMouse(QMouseEvent *event);
    void forwardKey(QKeyEvent *event);
    void scheduleRepaint();
    bool isTerminalWidget(QObject *object) const;
    QTermWidget *m_terminal;
    // Selection autoscroll: TerminalDisplay only scrolls when the mouse moves
    // past its edge, so replay the last drag position while it stays there.
    QPointer<QWidget> m_display;
    // The widget a press went to: it gets the moves and the release (Qt's
    // implicit grab, which an off-screen widget doesn't get by itself).
    QPointer<QWidget> m_mouseTarget;
    QTimer m_dragScrollTimer;
    QTimer m_repaintTimer;   // coalesces repaints to one per frame
    QTimer m_refreshTimer;   // catches repaints nothing announced
    QPointF m_dragPos;
    bool m_dragging = false;
    bool m_running;
};

// Registration function to be called from Rust (C linkage)
extern "C" void register_terminal_type();

#endif // TERMINAL_WIDGET_H
