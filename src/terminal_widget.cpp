#include "terminal_widget.h"
#include <QQmlEngine>
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

TerminalWidget::TerminalWidget(QObject *parent)
    : QObject(parent)
    , m_terminal(nullptr)
    , m_running(false)
{
    // Watch application-wide so keys delivered to the QML window can be
    // routed to the terminal (see m_keyboardCaptured).
    qApp->installEventFilter(this);
    m_dragScrollTimer.setInterval(50);
    connect(&m_dragScrollTimer, &QTimer::timeout, this, &TerminalWidget::dragScrollTick);
    setupTerminal();
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

    // Force native window creation
    m_terminal->setAttribute(Qt::WA_NativeWindow);
    m_terminal->winId();

    // Notify QML that the window handle changed
    QMetaObject::invokeMethod(this, "windowReady", Qt::QueuedConnection);
}

TerminalWidget::~TerminalWidget() {
    qApp->removeEventFilter(this);
    if (m_terminal) {
        delete m_terminal;
        m_terminal = nullptr;
    }
}

QWindow* TerminalWidget::window() const {
    if (m_terminal) {
        return m_terminal->windowHandle();
    }
    return nullptr;
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
}

bool TerminalWidget::forwardKeyboard(QObject *watched, QEvent *event) {
    if (!m_terminal || !m_display || !watched->isWindowType()) {
        return false;
    }
    // Events already delivered to the terminal's own window (X11, or a
    // compositor that does focus subsurfaces) need no help.
    QWindow *terminalWindow = m_terminal->windowHandle();
    if (watched == terminalWindow) {
        return false;
    }

    switch (event->type()) {
    case QEvent::MouseButtonPress:
        // Clicking anywhere else in the app window hands the keyboard back.
        m_keyboardCaptured = false;
        return false;
    case QEvent::KeyPress:
    case QEvent::KeyRelease:
        break;
    default:
        return false;
    }

    if (!m_keyboardCaptured || !m_terminal->isVisible()) {
        return false;
    }
    // Only keys aimed at the window that hosts the terminal.
    if (terminalWindow && terminalWindow->parent() && watched != terminalWindow->parent()) {
        return false;
    }

    auto *key = static_cast<QKeyEvent *>(event);
    QKeyEvent copy(key->type(), key->key(), key->modifiers(), key->nativeScanCode(),
                   key->nativeVirtualKey(), key->nativeModifiers(), key->text(),
                   key->isAutoRepeat(), key->count());
    QCoreApplication::sendEvent(m_display, &copy);
    return true;
}

bool TerminalWidget::eventFilter(QObject *watched, QEvent *event) {
    if (watched != m_display && forwardKeyboard(watched, event)) {
        return true;
    }
    if (watched == m_display && event->type() == QEvent::MouseButtonPress) {
        m_keyboardCaptured = true;
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
    menu.exec(m_terminal->mapToGlobal(pos));
}

void TerminalWidget::clear() {
    if (m_terminal) {
        m_terminal->clear();
    }
}

void TerminalWidget::show() {
    if (m_terminal) {
        m_terminal->show();
    }
}

void TerminalWidget::hide() {
    if (m_terminal) {
        m_terminal->hide();
    }
}

void TerminalWidget::focusTerminal() {
    if (!m_terminal) {
        return;
    }
    m_keyboardCaptured = true;

    // Activate the native window so the compositor delivers key events here.
    m_terminal->activateWindow();
    if (m_terminal->windowHandle()) {
        m_terminal->windowHandle()->requestActivate();
    }

    // QTermWidget delegates keyboard input to an internal TerminalDisplay child
    // widget. When the terminal is embedded as a foreign window via QML
    // WindowContainer, focusing the outer QTermWidget is not always enough to
    // route keystrokes to that inner widget, so focus it directly. Prefer the
    // focus proxy (set by QTermWidget to its display), then fall back to
    // locating the TerminalDisplay child by class name.
    QWidget *inner = m_terminal->focusProxy();
    if (!inner) {
        const QList<QWidget *> children = m_terminal->findChildren<QWidget *>();
        for (QWidget *child : children) {
            if (qstrcmp(child->metaObject()->className(), "TerminalDisplay") == 0) {
                inner = child;
                break;
            }
        }
    }

    if (inner) {
        inner->setFocus(Qt::OtherFocusReason);
    } else {
        m_terminal->setFocus(Qt::OtherFocusReason);
    }
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
    qDebug() << "TerminalWidget registered with QML";
}
