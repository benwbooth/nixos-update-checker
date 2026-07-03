#include "terminal_widget.h"
#include <QQmlEngine>
#include <QProcess>
#include <QDebug>
#include <QCoreApplication>

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

    int totalLines = m_terminal->screenLinesCount() + m_terminal->historyLinesCount();
    int totalCols = m_terminal->screenColumnsCount();

    m_terminal->setSelectionStart(0, 0);
    m_terminal->setSelectionEnd(totalLines, totalCols);
    QString allText = m_terminal->selectedText();
    // Clear selection
    m_terminal->setSelectionStart(0, 0);
    m_terminal->setSelectionEnd(0, 0);

    return allText;
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

    if (m_terminal->windowHandle()) {
        m_terminal->windowHandle()->requestActivate();
    }
    m_terminal->setFocus(Qt::OtherFocusReason);
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
