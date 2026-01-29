#include "terminal_widget.h"
#include <QQmlEngine>
#include <QProcess>
#include <QDebug>
#include <QCoreApplication>
#include <QFile>
#include <QFileInfo>
#include <QRegularExpression>
#include <QStandardPaths>
#include <QDir>
#include <QDateTime>
#include <QTextStream>

static QString debugLogPath() {
    QString dir = QStandardPaths::writableLocation(QStandardPaths::CacheLocation);
    QDir().mkpath(dir);
    return dir + "/terminal-debug.log";
}

static void debugLog(const QString &msg) {
    QFile f(debugLogPath());
    if (f.open(QIODevice::Append | QIODevice::Text)) {
        QTextStream out(&f);
        out << QDateTime::currentDateTime().toString(Qt::ISODate) << " " << msg << "\n";
    }
}

TerminalWidget::TerminalWidget(QObject *parent)
    : QObject(parent)
    , m_terminal(nullptr)
    , m_pollTimer(new QTimer(this))
    , m_running(false)
{
    // Timer to poll terminal content for status line updates
    m_pollTimer->setInterval(200); // Poll every 200ms
    connect(m_pollTimer, &QTimer::timeout, this, &TerminalWidget::pollLastLine);

    setupTerminal();
}

void TerminalWidget::setupTerminal() {
    // Destroy old terminal if it exists
    if (m_terminal) {
        disconnect(m_terminal, &QTermWidget::finished, this, &TerminalWidget::onFinished);
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

    // Set environment to support colors
    QStringList env = QProcess::systemEnvironment();
    env << "TERM=xterm-256color";
    m_terminal->setEnvironment(env);

    // Connect signals
    connect(m_terminal, &QTermWidget::finished, this, &TerminalWidget::onFinished);

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

void TerminalWidget::runCommand(const QString &command) {
    if (!m_terminal) return;

    m_running = true;
    emit runningChanged();

    // Start a shell and send the command
    m_terminal->startShellProgram();
    m_terminal->sendText(command + "\n");
}

void TerminalWidget::runScript(const QString &flakePath, const QString &commitMsg) {
    // Recreate the terminal widget for a clean session.
    // QTermWidget doesn't reliably support restarting after a session ends.
    setupTerminal();

    m_running = true;
    m_pollTimer->start(); // Start polling for status line updates
    emit runningChanged();

    // Set SHELL env var for pkexec security check
    QStringList env = QProcess::systemEnvironment();
    env << "SHELL=/bin/sh";
    env << "TERM=xterm-256color";
    m_terminal->setEnvironment(env);

    // Find the script - check dev location first, then installed location
    QString scriptPath = "/run/current-system/sw/bin/nixos-update-checker-update";
    QString devScriptPath = QCoreApplication::applicationDirPath() + "/../../scripts/nixos-update-checker-update";
    if (QFile::exists(devScriptPath)) {
        scriptPath = QFileInfo(devScriptPath).canonicalFilePath();
    }

    // Get SSH_AUTH_SOCK to pass through for git push
    QString sshAuthSock = qEnvironmentVariable("SSH_AUTH_SOCK");

    // Run pkexec with env to pass variables
    m_terminal->setShellProgram("/bin/sh");
    m_terminal->setArgs({"-c",
        QString("pkexec env FLAKE_PATH='%1' COMMIT_MSG='%2' SSH_AUTH_SOCK='%3' '%4'")
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
    debugLog("TerminalWidget::show() called");
    if (m_terminal) {
        debugLog(QString("  Before show - isVisible:%1 isHidden:%2").arg(m_terminal->isVisible()).arg(m_terminal->isHidden()));
        if (m_terminal->windowHandle()) {
            debugLog(QString("  Before show - windowHandle visible:%1").arg(m_terminal->windowHandle()->isVisible()));
        }
        m_terminal->show();
        debugLog(QString("  After show - isVisible:%1 isHidden:%2").arg(m_terminal->isVisible()).arg(m_terminal->isHidden()));
        if (m_terminal->windowHandle()) {
            debugLog(QString("  After show - windowHandle visible:%1").arg(m_terminal->windowHandle()->isVisible()));
        }
    }
}

void TerminalWidget::hide() {
    if (m_terminal) {
        m_terminal->hide();
    }
}

void TerminalWidget::log(const QString &msg) {
    debugLog(msg);
}

bool TerminalWidget::hasContent() const {
    if (!m_terminal) return false;
    int lines = m_terminal->screenLinesCount() + m_terminal->historyLinesCount();
    return lines > 1;
}

void TerminalWidget::onFinished() {
    debugLog("=== TerminalWidget::onFinished() START ===");
    debugLog(QString("  Log file: %1").arg(debugLogPath()));

    if (m_terminal) {
        debugLog(QString("  m_terminal->isVisible():%1 isHidden:%2").arg(m_terminal->isVisible()).arg(m_terminal->isHidden()));
        debugLog(QString("  m_terminal->windowHandle():%1").arg((quintptr)m_terminal->windowHandle(), 0, 16));
        if (m_terminal->windowHandle()) {
            debugLog(QString("  windowHandle()->isVisible():%1 isExposed:%2").arg(m_terminal->windowHandle()->isVisible()).arg(m_terminal->windowHandle()->isExposed()));
        }
    }

    m_pollTimer->stop();
    pollLastLine();
    m_running = false;
    emit runningChanged();

    debugLog("  After runningChanged:");
    if (m_terminal) {
        debugLog(QString("  isVisible:%1 isHidden:%2").arg(m_terminal->isVisible()).arg(m_terminal->isHidden()));
        if (m_terminal->windowHandle()) {
            debugLog(QString("  windowHandle visible:%1").arg(m_terminal->windowHandle()->isVisible()));
        } else {
            debugLog("  windowHandle() is NULL!");
        }
    }

    int exitCode = 0;
    if (m_terminal) {
        int totalLines = m_terminal->screenLinesCount() + m_terminal->historyLinesCount();
        int totalCols = m_terminal->screenColumnsCount();
        m_terminal->setSelectionStart(0, 0);
        m_terminal->setSelectionEnd(totalLines, totalCols);
        QString allText = m_terminal->selectedText();
        m_terminal->setSelectionStart(0, 0);
        m_terminal->setSelectionEnd(0, 0);

        if (allText.contains("Update failed") || allText.contains("✗")) {
            exitCode = 1;
        }
        debugLog(QString("  exitCode:%1").arg(exitCode));
    }

    debugLog("  About to emit finished()");
    emit finished(exitCode);

    debugLog("  After emit finished():");
    if (m_terminal) {
        debugLog(QString("  isVisible:%1 isHidden:%2").arg(m_terminal->isVisible()).arg(m_terminal->isHidden()));
        if (m_terminal->windowHandle()) {
            debugLog(QString("  windowHandle visible:%1").arg(m_terminal->windowHandle()->isVisible()));
        } else {
            debugLog("  windowHandle() is NULL after finished!");
        }
    }
    debugLog("=== TerminalWidget::onFinished() END ===");

    // Delayed checks to catch async changes
    QTimer::singleShot(100, this, [this]() {
        debugLog("=== Delayed check (100ms) ===");
        if (m_terminal) {
            debugLog(QString("  isVisible:%1 isHidden:%2").arg(m_terminal->isVisible()).arg(m_terminal->isHidden()));
            if (m_terminal->windowHandle()) {
                debugLog(QString("  windowHandle visible:%1 exposed:%2").arg(m_terminal->windowHandle()->isVisible()).arg(m_terminal->windowHandle()->isExposed()));
            } else {
                debugLog("  windowHandle() is NULL!");
            }
        } else {
            debugLog("  m_terminal is NULL!");
        }
    });

    QTimer::singleShot(500, this, [this]() {
        debugLog("=== Delayed check (500ms) ===");
        if (m_terminal) {
            debugLog(QString("  isVisible:%1 isHidden:%2").arg(m_terminal->isVisible()).arg(m_terminal->isHidden()));
            if (m_terminal->windowHandle()) {
                debugLog(QString("  windowHandle visible:%1 exposed:%2").arg(m_terminal->windowHandle()->isVisible()).arg(m_terminal->windowHandle()->isExposed()));
            } else {
                debugLog("  windowHandle() is NULL!");
            }
        } else {
            debugLog("  m_terminal is NULL!");
        }
    });

    QTimer::singleShot(2000, this, [this]() {
        debugLog("=== Delayed check (2000ms) ===");
        if (m_terminal) {
            debugLog(QString("  isVisible:%1 isHidden:%2").arg(m_terminal->isVisible()).arg(m_terminal->isHidden()));
            if (m_terminal->windowHandle()) {
                debugLog(QString("  windowHandle visible:%1 exposed:%2").arg(m_terminal->windowHandle()->isVisible()).arg(m_terminal->windowHandle()->isExposed()));
            } else {
                debugLog("  windowHandle() is NULL!");
            }
        } else {
            debugLog("  m_terminal is NULL!");
        }
    });
}

void TerminalWidget::pollLastLine() {
    if (!m_terminal) return;

    // Get the last non-empty line from the terminal screen
    // Use setSelectionStart/End to select all text, then get selectedText
    int totalLines = m_terminal->screenLinesCount() + m_terminal->historyLinesCount();
    int totalCols = m_terminal->screenColumnsCount();

    m_terminal->setSelectionStart(0, 0);
    m_terminal->setSelectionEnd(totalLines, totalCols);
    QString allText = m_terminal->selectedText();
    // Clear selection by setting start after end
    m_terminal->setSelectionStart(0, 0);
    m_terminal->setSelectionEnd(0, 0);

    if (!allText.isEmpty()) {
        // Split into lines and find the last non-empty line
        QStringList lines = allText.split('\n', Qt::SkipEmptyParts);
        if (!lines.isEmpty()) {
            QString lastLine = lines.last().trimmed();
            // Filter out ANSI escape sequences for display
            lastLine.remove(QRegularExpression("\x1b\\[[0-9;]*m"));
            if (!lastLine.isEmpty() && lastLine != m_lastLine) {
                m_lastLine = lastLine;
                emit lastLineChanged();
            }
        }
    }
}

// Register the type with QML - use C linkage so Rust can call it
extern "C" void register_terminal_type() {
    qmlRegisterType<TerminalWidget>("Terminal", 1, 0, "TerminalWidget");
    qDebug() << "TerminalWidget registered with QML";
}
