#ifndef TERMINAL_WIDGET_H
#define TERMINAL_WIDGET_H

#include <QObject>
#include <QWindow>
#include <QWidget>
#include <qtermwidget6/qtermwidget.h>

class TerminalWidget : public QObject {
    Q_OBJECT
    Q_PROPERTY(QWindow* window READ window NOTIFY windowReady)
    Q_PROPERTY(bool running READ running WRITE setRunning NOTIFY runningChanged)

public:
    explicit TerminalWidget(QObject *parent = nullptr);
    ~TerminalWidget();

    QWindow* window() const;
    bool running() const { return m_running; }
    void setRunning(bool running);

    Q_INVOKABLE void runCommand(const QString &command);
    Q_INVOKABLE QString getAllText();
    Q_INVOKABLE void clear();
    Q_INVOKABLE void show();
    Q_INVOKABLE void hide();
    Q_INVOKABLE void focusTerminal();
    Q_INVOKABLE void log(const QString &msg);
    Q_INVOKABLE bool hasContent() const;
    Q_INVOKABLE void copySelection();
    Q_INVOKABLE void paste();
    Q_INVOKABLE void selectAll();

signals:
    void windowReady();
    void runningChanged();

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    void setupTerminal();
    void showContextMenu(const QPoint &pos);
    QTermWidget *m_terminal;
    bool m_running;
};

// Registration function to be called from Rust (C linkage)
extern "C" void register_terminal_type();

#endif // TERMINAL_WIDGET_H
