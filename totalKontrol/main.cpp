#include <signal.h>
#include <QApplication>
#include "mainwindow.h"

bool done;




int main(int argc, char *argv[])
{
    QApplication a(argc, argv);

    MainWindow window;
    window.show();
    return a.exec();
}
