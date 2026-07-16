#include <QApplication>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QVBoxLayout>
#include <QWidget>

int main(int argc, char **argv) {
    if (qEnvironmentVariableIsEmpty("QT_QPA_PLATFORM")) {
        qputenv("QT_QPA_PLATFORM", QByteArray("wayland"));
    }

    QApplication app(argc, argv);

    QWidget window;
    window.setWindowTitle("ReduxOS Wayland Qt Demo");
    window.resize(640, 360);

    auto *layout = new QVBoxLayout(&window);

    auto *title = new QLabel("Qt Widgets on Wayland");
    QFont font = title->font();
    font.setPointSize(16);
    font.setBold(true);
    title->setFont(font);
    title->setAlignment(Qt::AlignLeft | Qt::AlignVCenter);

    auto *status = new QLabel("Si ves esta ventana, Qt se conecto a wayland-0.");
    status->setWordWrap(true);

    auto *hint = new QLabel("Tip: prueba mover, enfocar y cerrar.");
    hint->setWordWrap(true);

    auto *row = new QHBoxLayout();
    row->addStretch(1);
    auto *closeButton = new QPushButton("Cerrar");
    QObject::connect(closeButton, &QPushButton::clicked, &window, &QWidget::close);
    row->addWidget(closeButton);

    layout->addWidget(title);
    layout->addWidget(status);
    layout->addWidget(hint);
    layout->addStretch(1);
    layout->addLayout(row);

    window.show();
    return app.exec();
}
