#include "terminal_widget.h"
#include <QQmlEngine>
#include <QProcess>
#include <QDebug>
#include <QCoreApplication>
#include <QFile>
#include <QFileInfo>

TerminalWidget::TerminalWidget(QObject *parent)
    : QObject(parent)
    , m_terminal(new QTermWidget(0)) // 0 = don't start shell automatically
    , m_running(false)
{
    // Configure terminal appearance
    m_terminal->setScrollBarPosition(QTermWidget::ScrollBarRight);
    m_terminal->setColorScheme("Linux");
    m_terminal->setTerminalFont(QFont("Monospace", 10));
    m_terminal->setTerminalOpacity(1.0);

    // Set environment to support colors
    QStringList env = QProcess::systemEnvironment();
    env << "TERM=xterm-256color";
    m_terminal->setEnvironment(env);

    // Connect signals
    connect(m_terminal, &QTermWidget::finished, this, &TerminalWidget::onFinished);

    // Force native window creation
    m_terminal->setAttribute(Qt::WA_NativeWindow);
    m_terminal->winId();

    // Emit window ready after creation
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

void TerminalWidget::runCommand(const QString &command) {
    if (!m_terminal) return;

    m_running = true;
    emit runningChanged();

    // Start a shell and send the command
    m_terminal->startShellProgram();
    m_terminal->sendText(command + "\n");
}

void TerminalWidget::runScript(const QString &flakePath, const QString &commitMsg) {
    if (!m_terminal) return;

    m_running = true;
    emit runningChanged();

    // Set SHELL env var for pkexec security check
    QStringList env = QProcess::systemEnvironment();
    env << "SHELL=/bin/sh";
    m_terminal->setEnvironment(env);

    // Find the script - check dev location first, then installed location
    QString scriptPath = "/run/current-system/sw/bin/nixos-update-checker-update";
    QString devScriptPath = QCoreApplication::applicationDirPath() + "/../../scripts/nixos-update-checker-update";
    if (QFile::exists(devScriptPath)) {
        scriptPath = QFileInfo(devScriptPath).canonicalFilePath();
    }

    // Get SSH_AUTH_SOCK to pass through for git push
    QString sshAuthSock = qEnvironmentVariable("SSH_AUTH_SOCK");

    // Run pkexec with env to pass variables (including SSH_AUTH_SOCK for git push)
    m_terminal->setShellProgram("/bin/sh");
    m_terminal->setArgs({"-c",
        QString("pkexec env FLAKE_PATH='%1' COMMIT_MSG='%2' SSH_AUTH_SOCK='%3' '%4'; echo ''; echo '=== Press Enter to close ===' && read")
            .arg(flakePath)
            .arg(commitMsg)
            .arg(sshAuthSock)
            .arg(scriptPath)
    });
    m_terminal->startShellProgram();
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

void TerminalWidget::onFinished() {
    m_running = false;
    emit runningChanged();
    emit finished(0);
}

// Register the type with QML - use C linkage so Rust can call it
extern "C" void register_terminal_type() {
    qmlRegisterType<TerminalWidget>("Terminal", 1, 0, "TerminalWidget");
    qDebug() << "TerminalWidget registered with QML";
}
