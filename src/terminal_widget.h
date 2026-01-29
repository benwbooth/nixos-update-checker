#ifndef TERMINAL_WIDGET_H
#define TERMINAL_WIDGET_H

#include <QObject>
#include <QWindow>
#include <QWidget>
#include <QTimer>
#include <qtermwidget6/qtermwidget.h>

class TerminalWidget : public QObject {
    Q_OBJECT
    Q_PROPERTY(QWindow* window READ window NOTIFY windowReady)
    Q_PROPERTY(bool running READ running NOTIFY runningChanged)
    Q_PROPERTY(QString lastLine READ lastLine NOTIFY lastLineChanged)

public:
    explicit TerminalWidget(QObject *parent = nullptr);
    ~TerminalWidget();

    QWindow* window() const;
    bool running() const { return m_running; }
    QString lastLine() const { return m_lastLine; }

    Q_INVOKABLE void runCommand(const QString &command);
    Q_INVOKABLE void runScript(const QString &flakePath, const QString &commitMsg);
    Q_INVOKABLE void clear();
    Q_INVOKABLE void show();
    Q_INVOKABLE void hide();
    Q_INVOKABLE void log(const QString &msg);
    Q_INVOKABLE bool hasContent() const;

signals:
    void finished(int exitCode);
    void windowReady();
    void runningChanged();
    void lastLineChanged();
    void outputReceived(const QString &text);

private slots:
    void onFinished();
    void pollLastLine();

private:
    void setupTerminal();
    QTermWidget *m_terminal;
    QTimer *m_pollTimer;
    bool m_running;
    QString m_lastLine;
};

// Registration function to be called from Rust (C linkage)
extern "C" void register_terminal_type();

#endif // TERMINAL_WIDGET_H
