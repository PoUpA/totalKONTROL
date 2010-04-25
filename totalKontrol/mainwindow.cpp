#include "mainwindow.h"
#include "ui_mainwindow.h"
#include <QtGui>
#include <QStringList>
#include "midikontrol.h"
#include "padkontrol.h"
#include "midilistener.h"

MainWindow::MainWindow(QWidget *parent) :
    QMainWindow(parent),
    ui(new Ui::MainWindow)
{
    ui->setupUi(this);
    QStateMachine machine;

    MidiKontrol *midiKontrol = new MidiKontrol();
    QGroupBox *deviceInGroup = new QGroupBox(tr("Device In"));
    QLabel *deviceInLabel = new QLabel(tr("PadKontrol PORT A"));
    QComboBox *deviceInComboBox = new QComboBox;
    deviceInComboBox->addItems(midiKontrol->getInputDevicesList());

    QGroupBox *deviceOutGroup = new QGroupBox(tr("Device Out"));
    QLabel *deviceOutLabel = new QLabel(tr("PadKontrol CTRL"));
    QComboBox *deviceOutComboBox = new QComboBox;
    deviceOutComboBox->addItems(midiKontrol->getOutputDevicesList());


    QGridLayout *deviceOutLayout = new QGridLayout;
    deviceOutLayout->addWidget(deviceOutLabel, 0, 0);
    deviceOutLayout->addWidget(deviceOutComboBox, 0, 1);
    deviceOutGroup->setLayout(deviceOutLayout);

    QGridLayout *deviceInLayout = new QGridLayout;
    deviceInLayout->addWidget(deviceInLabel, 0, 0);
    deviceInLayout->addWidget(deviceInComboBox, 0, 1);
    deviceInGroup->setLayout(deviceInLayout);


    QPushButton *nativeModeOnBtn = new QPushButton("Enter Native mode", this);
    QPushButton *nativeModeOffBtn = new QPushButton("Exit Native mode", this);
    QLabel *padMessage = new QLabel;
    QGridLayout *layout = new QGridLayout;
    layout->addWidget(deviceOutGroup, 0, 0);
    layout->addWidget(deviceInGroup, 0, 1);
    layout->addWidget(nativeModeOnBtn,1,0);
    layout->addWidget(nativeModeOffBtn,1,1);
    layout->addWidget(padMessage,2,1);
    centralWidget()->setLayout(layout);
    PadKontrol *padKontrol = new PadKontrol();
    connect(deviceInComboBox,
            SIGNAL(currentIndexChanged(int)),
            padKontrol,
            SLOT(connectPadIn(int)));
    connect(deviceOutComboBox,
            SIGNAL(currentIndexChanged(int)),
            padKontrol,
            SLOT(connectPadOut(int)));
    connect(nativeModeOnBtn,
            SIGNAL(clicked()),
            padKontrol,
            SLOT(enterNativeMode()));
    connect(nativeModeOffBtn,
            SIGNAL(clicked()),
            padKontrol,
            SLOT(exitNativeMode()));
    setWindowTitle(tr("totalKontrol"));

    connect(padKontrol,SIGNAL(midiMessage(QString)),padMessage,SLOT(setText(QString)));

}

MainWindow::~MainWindow()
{
    delete ui;
}

void MainWindow::changeEvent(QEvent *e)
{
    QMainWindow::changeEvent(e);
    switch (e->type()) {
    case QEvent::LanguageChange:
        ui->retranslateUi(this);
        break;
    default:
        break;
    }
}


