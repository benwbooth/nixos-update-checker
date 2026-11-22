#include "terminal_widget.h"
#include "global_menu.h"
#include <QQmlEngine>
#include <QQuickWindow>
#include <QProcess>
#include <QDebug>
#include <QCoreApplication>
#include <QGuiApplication>
#include <QBuffer>
#include <QKeyEvent>
#include <QMenu>
#include <QMouseEvent>
#include <QScrollBar>
#include <QWheelEvent>
#include <QCursor>
#include <QPainter>
#include <QFocusEvent>

TerminalWidget::TerminalWidget(QQuickItem *parent)
    : QQuickPaintedItem(parent)
    , m_terminal(nullptr)
    , m_running(false)
{
    // Application-wide: sees the terminal's own events (shortcuts, drag
    // scrolling, wheel) and its repaint requests.
    qApp->installEventFilter(this);
    setAcceptedMouseButtons(Qt::AllButtons);
    setFlag(ItemIsFocusScope, false);
    setFlag(ItemAcceptsInputMethod, true);
    setCursor(Qt::IBeamCursor);
    setOpaquePainting(true);
    setFillColor(QColor("#000000"));
    m_dragScrollTimer.setInterval(50);
    connect(&m_dragScrollTimer, &QTimer::timeout, this, &TerminalWidget::dragScrollTick);
    m_repaintTimer.setSingleShot(true);
    m_repaintTimer.setInterval(8);
    connect(&m_repaintTimer, &QTimer::timeout, this, [this] { update(); });
    m_refreshTimer.setInterval(100);
    connect(&m_refreshTimer, &QTimer::timeout, this, [this] { update(); });
    setupTerminal();
}

void TerminalWidget::scheduleRepaint() {
    if (isVisible() && !m_repaintTimer.isActive()) m_repaintTimer.start();
}

bool TerminalWidget::isTerminalWidget(QObject *object) const {
    for (QObject *o = object; o; o = o->parent()) {
        if (o == m_terminal) return true;
    }
    return false;
}

void TerminalWidget::paint(QPainter *painter) {
    if (!m_terminal) return;
    m_terminal->render(painter, QPoint(), QRegion(),
                       QWidget::DrawWindowBackground | QWidget::DrawChildren);
}

void TerminalWidget::geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) {
    QQuickPaintedItem::geometryChange(newGeometry, oldGeometry);
    if (m_terminal && newGeometry.size() != oldGeometry.size() && !newGeometry.isEmpty()) {
        m_terminal->resize(newGeometry.size().toSize());
        update();
    }
}

void TerminalWidget::itemChange(ItemChange change, const ItemChangeData &value) {
    QQuickPaintedItem::itemChange(change, value);
    if (change == ItemVisibleHasChanged) {
        if (value.boolValue) {
            m_refreshTimer.start();
            update();
        } else {
            m_refreshTimer.stop();
        }
    }
}

void TerminalWidget::forwardMouse(QMouseEvent *event) {
    if (!m_terminal) return;
    const QPointF pos = event->position();
    QWidget *target = m_mouseTarget;
    if (!target || event->type() == QEvent::MouseButtonPress || event->type() == QEvent::MouseButtonDblClick) {
        target = m_terminal->childAt(pos.toPoint());
        if (!target) target = m_terminal;
        if (event->type() == QEvent::MouseButtonPress) m_mouseTarget = target;
    }
    const QPointF local = target->mapFrom(m_terminal, pos);
    QMouseEvent copy(event->type(), local, local, mapToGlobal(pos), event->button(),
                     event->buttons(), event->modifiers());
    QCoreApplication::sendEvent(target, &copy);
    if (event->type() == QEvent::MouseButtonRelease && event->buttons() == Qt::NoButton) {
        m_mouseTarget = nullptr;
    }
    event->accept();
    scheduleRepaint();
}

void TerminalWidget::mousePressEvent(QMouseEvent *event) {
    forceActiveFocus(Qt::MouseFocusReason);
    forwardMouse(event);
}

void TerminalWidget::mouseMoveEvent(QMouseEvent *event) { forwardMouse(event); }
void TerminalWidget::mouseReleaseEvent(QMouseEvent *event) { forwardMouse(event); }
void TerminalWidget::mouseDoubleClickEvent(QMouseEvent *event) { forwardMouse(event); }

void TerminalWidget::wheelEvent(QWheelEvent *event) {
    if (!m_terminal) return;
    const QPointF pos = event->position();
    QWidget *target = m_terminal->childAt(pos.toPoint());
    if (!target) target = m_terminal;
    const QPointF local = target->mapFrom(m_terminal, pos);
    QWheelEvent copy(local, mapToGlobal(pos), event->pixelDelta(), event->angleDelta(),
                     event->buttons(), event->modifiers(), event->phase(), event->inverted());
    QCoreApplication::sendEvent(target, &copy);
    event->accept();
    scheduleRepaint();
}

void TerminalWidget::forwardKey(QKeyEvent *event) {
    if (!m_display) return;
    // Sent through the application's event filters, so eventFilter() still
    // turns Ctrl+Shift+C/V/A into copy, paste and select all.
    QKeyEvent copy(event->type(), event->key(), event->modifiers(), event->nativeScanCode(),
                   event->nativeVirtualKey(), event->nativeModifiers(), event->text(),
                   event->isAutoRepeat(), event->count());
    QCoreApplication::sendEvent(m_display, &copy);
    event->accept();
    scheduleRepaint();
}

void TerminalWidget::keyPressEvent(QKeyEvent *event) { forwardKey(event); }
void TerminalWidget::keyReleaseEvent(QKeyEvent *event) { forwardKey(event); }

void TerminalWidget::focusInEvent(QFocusEvent *event) {
    QQuickPaintedItem::focusInEvent(event);
    if (m_display) {
        QFocusEvent in(QEvent::FocusIn, event->reason());
        QCoreApplication::sendEvent(m_display, &in);
    }
    scheduleRepaint();
}

void TerminalWidget::focusOutEvent(QFocusEvent *event) {
    QQuickPaintedItem::focusOutEvent(event);
    if (m_display) {
        QFocusEvent out(QEvent::FocusOut, event->reason());
        QCoreApplication::sendEvent(m_display, &out);
    }
    scheduleRepaint();
}

void TerminalWidget::setupTerminal() {
    // Destroy old terminal if it exists
    if (m_terminal) {
        delete m_terminal;
        m_terminal = nullptr;
    }

    m_terminal = new QTermWidget(0); // 0 = don't start shell automatically

    // Configure terminal appearance
    m_terminal->setScrollBarPosition(QTermWidget::ScrollBarRight);
    // Keep plenty of scrollback so the wheel has output to scroll through.
    m_terminal->setHistorySize(10000);
    m_terminal->setColorScheme("Linux");
    m_terminal->setTerminalFont(QFont("Monospace", 10));
    m_terminal->setTerminalOpacity(1.0);
    // Keep the widget/window alive after the shell exits so content persists.
    m_terminal->setAutoClose(false);
    m_terminal->setFocusPolicy(Qt::StrongFocus);

    // Copy/paste: QTermWidget leaves shortcuts and the context menu to the
    // embedding app. Rectangular selection is built in (Ctrl+Alt+drag).
    m_terminal->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(m_terminal, &QWidget::customContextMenuRequested,
            this, &TerminalWidget::showContextMenu);
    m_dragScrollTimer.stop();
    m_dragging = false;
    // The application-wide filter installed in the constructor sees the
    // display's events too; a second per-widget filter would run them twice.
    m_display = m_terminal->focusProxy();

    // Set environment to support colors
    QStringList env = QProcess::systemEnvironment();
    env << "TERM=xterm-256color";
    m_terminal->setEnvironment(env);

    m_terminal->setBlinkingCursor(false);
    // Laid out and painted as if shown, but never on screen: this item
    // draws it (see the class comment).
    m_terminal->setAttribute(Qt::WA_DontShowOnScreen);
    if (width() > 0 && height() > 0) {
        m_terminal->resize(size().toSize());
    } else {
        m_terminal->resize(800, 400);
    }
    m_terminal->show();
    connect(m_terminal, &QTermWidget::receivedData, this, &TerminalWidget::scheduleRepaint);
    m_mouseTarget = nullptr;
    update();
}

TerminalWidget::~TerminalWidget() {
    qApp->removeEventFilter(this);
    if (m_terminal) {
        delete m_terminal;
        m_terminal = nullptr;
    }
}

void TerminalWidget::setRunning(bool running) {
    if (m_running != running) {
        m_running = running;
        emit runningChanged();
    }
}

void TerminalWidget::runCommand(const QString &command) {
    if (!m_terminal) return;

    // Recreate the terminal widget for a clean session
    setupTerminal();

    m_running = true;
    emit runningChanged();

    // Set SHELL env var for pkexec security check
    QStringList env = QProcess::systemEnvironment();
    env << "SHELL=/bin/sh";
    env << "TERM=xterm-256color";
    m_terminal->setEnvironment(env);

    m_terminal->setShellProgram("/bin/sh");
    m_terminal->setArgs({"-c", command});
    m_terminal->startShellProgram();
    QMetaObject::invokeMethod(this, "focusTerminal", Qt::QueuedConnection);
    update();
}

QString TerminalWidget::getAllText() {
    if (!m_terminal) return QString();

    // Read via saveHistory rather than the selection API so polling doesn't
    // clobber whatever the user has selected.
    QBuffer buffer;
    buffer.open(QIODevice::WriteOnly);
    m_terminal->saveHistory(&buffer);
    return QString::fromUtf8(buffer.data());
}

void TerminalWidget::copySelection() {
    if (m_terminal) {
        m_terminal->copyClipboard();
    }
}

void TerminalWidget::paste() {
    if (m_terminal) {
        m_terminal->pasteClipboard();
    }
}

void TerminalWidget::selectAll() {
    if (!m_terminal) return;
    int totalLines = m_terminal->screenLinesCount() + m_terminal->historyLinesCount();
    m_terminal->setSelectionStart(0, 0);
    m_terminal->setSelectionEnd(totalLines, m_terminal->screenColumnsCount());
}

bool TerminalWidget::handleDragScroll(QWidget *display, QEvent *event) {
    switch (event->type()) {
    case QEvent::MouseButtonPress: {
        auto *mouse = static_cast<QMouseEvent *>(event);
        if (mouse->button() == Qt::LeftButton) {
            m_dragging = true;
            m_dragPos = mouse->position();
        }
        break;
    }
    case QEvent::MouseMove: {
        auto *mouse = static_cast<QMouseEvent *>(event);
        if (!m_dragging || !(mouse->buttons() & Qt::LeftButton)) break;
        m_dragPos = mouse->position();
        const bool outside = m_dragPos.y() < 0 || m_dragPos.y() >= display->height();
        if (outside && !m_dragScrollTimer.isActive()) m_dragScrollTimer.start();
        else if (!outside) m_dragScrollTimer.stop();
        break;
    }
    case QEvent::MouseButtonRelease:
        if (static_cast<QMouseEvent *>(event)->button() == Qt::LeftButton) {
            m_dragging = false;
            m_dragScrollTimer.stop();
        }
        break;
    case QEvent::Wheel:
        // Scrolling with the wheel mid-drag moves the view but not the
        // selection end; replay the drag position once the view has moved.
        if (m_dragging) QTimer::singleShot(0, this, &TerminalWidget::dragScrollTick);
        break;
    default:
        break;
    }
    return false;
}

bool TerminalWidget::swallowWheel() const {
    // With nothing in the scrollback, TerminalDisplay turns the wheel into
    // Up/Down key presses for pagers like less. Update commands never read
    // them, so the tty just echoes ^[[A / ^[[B into the output. Drop the
    // wheel instead; once there is history it scrolls the view as usual.
    if (!m_terminal) {
        return false;
    }
    auto *bar = m_terminal->findChild<QScrollBar *>();
    return bar && bar->maximum() == 0;
}

void TerminalWidget::dragScrollTick() {
    if (!m_display || !m_dragging || !(QGuiApplication::mouseButtons() & Qt::LeftButton)) {
        m_dragging = false;
        m_dragScrollTimer.stop();
        return;
    }
    // A synthetic move makes TerminalDisplay::extendSelection scroll one step
    // further and stretch the selection to the newly revealed line.
    QMouseEvent move(QEvent::MouseMove, m_dragPos, m_display->mapToGlobal(m_dragPos),
                     Qt::NoButton, Qt::LeftButton, QGuiApplication::keyboardModifiers());
    QCoreApplication::sendEvent(m_display, &move);
    scheduleRepaint();
}

bool TerminalWidget::eventFilter(QObject *watched, QEvent *event) {
    // The off-screen terminal asks to be repainted (cursor, selection,
    // scrolling): draw this item again.
    if ((event->type() == QEvent::UpdateRequest || event->type() == QEvent::UpdateLater
         || event->type() == QEvent::Paint) && isTerminalWidget(watched)) {
        scheduleRepaint();
    }
    if (watched != m_display) {
        return QObject::eventFilter(watched, event);
    }
    if (handleDragScroll(m_display, event)) {
        return true;
    }
    if (event->type() == QEvent::Wheel && swallowWheel()) {
        return true;
    }
    if (event->type() != QEvent::KeyPress && event->type() != QEvent::ShortcutOverride) {
        return QObject::eventFilter(watched, event);
    }

    auto *key = static_cast<QKeyEvent *>(event);
    const auto mods = key->modifiers() & (Qt::ControlModifier | Qt::ShiftModifier | Qt::AltModifier);
    bool isCopy = (mods == (Qt::ControlModifier | Qt::ShiftModifier) && key->key() == Qt::Key_C)
               || (mods == Qt::ControlModifier && key->key() == Qt::Key_Insert);
    bool isPaste = (mods == (Qt::ControlModifier | Qt::ShiftModifier) && key->key() == Qt::Key_V)
                || (mods == Qt::ShiftModifier && key->key() == Qt::Key_Insert);
    bool isSelectAll = mods == (Qt::ControlModifier | Qt::ShiftModifier) && key->key() == Qt::Key_A;

    if (!isCopy && !isPaste && !isSelectAll) {
        return QObject::eventFilter(watched, event);
    }
    if (event->type() == QEvent::ShortcutOverride) {
        event->accept();
        return true;
    }

    if (isCopy) copySelection();
    else if (isPaste) paste();
    else selectAll();
    return true;
}

void TerminalWidget::showContextMenu(const QPoint &pos) {
    if (!m_terminal) return;

    QMenu menu;
    QAction *copy = menu.addAction(QIcon::fromTheme("edit-copy"), tr("Copy"), this,
                                   &TerminalWidget::copySelection);
    copy->setShortcut(QKeySequence("Ctrl+Shift+C"));
    copy->setEnabled(!m_terminal->selectedText().isEmpty());
    QAction *pasteAction = menu.addAction(QIcon::fromTheme("edit-paste"), tr("Paste"), this,
                                          &TerminalWidget::paste);
    pasteAction->setShortcut(QKeySequence("Ctrl+Shift+V"));
    pasteAction->setEnabled(m_running);
    menu.addSeparator();
    QAction *all = menu.addAction(QIcon::fromTheme("edit-select-all"), tr("Select All"), this,
                                  &TerminalWidget::selectAll);
    all->setShortcut(QKeySequence("Ctrl+Shift+A"));
    menu.addSeparator();
    QAction *hint = menu.addAction(tr("Tip: Ctrl+Alt+drag for block selection"));
    hint->setEnabled(false);
    Q_UNUSED(pos);
    // The terminal is off screen: open the menu at the pointer.
    menu.exec(QCursor::pos());
}

void TerminalWidget::clear() {
    if (m_terminal) {
        m_terminal->clear();
    }
}

void TerminalWidget::focusTerminal() {
    if (!m_terminal) return;
    forceActiveFocus(Qt::OtherFocusReason);
}

void TerminalWidget::log(const QString &msg) {
    qDebug() << msg;
}

bool TerminalWidget::hasContent() const {
    if (!m_terminal) return false;
    int lines = m_terminal->screenLinesCount() + m_terminal->historyLinesCount();
    return lines > 1;
}

// Register the type with QML - use C linkage so Rust can call it
extern "C" void register_terminal_type() {
    qmlRegisterType<TerminalWidget>("Terminal", 1, 0, "TerminalWidget");
    qmlRegisterType<GlobalMenuBar>("GlobalMenu", 1, 0, "GlobalMenuBar");
    qDebug() << "TerminalWidget registered with QML";
}

// Native (FreeType) text for every window and Text item, with the desktop's
// hinting/antialiasing, instead of Qt Quick's distance-field glyphs, which
// look soft at fractional scales (133%). Call before any window exists.
extern "C" void use_native_text_rendering() {
    QQuickWindow::setTextRenderType(QQuickWindow::NativeTextRendering);
}
