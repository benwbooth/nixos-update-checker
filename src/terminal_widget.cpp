#include "terminal_widget.h"
#include <QQmlEngine>
#include <QProcess>
#include <QDebug>
#include <QCoreApplication>
#include <QBuffer>
#include <QKeyEvent>
#include <QMenu>

TerminalWidget::TerminalWidget(QObject *parent)
    : QObject(parent)
    , m_terminal(nullptr)
    , m_running(false)
{
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
    if (QWidget *display = m_terminal->focusProxy()) {
        display->installEventFilter(this);
    }

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

bool TerminalWidget::eventFilter(QObject *watched, QEvent *event) {
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
