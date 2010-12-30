#include "mainwindow.h"
#include "ui_mainwindow.h"
#include <QtGui>
#include <QStringList>
#include "midikontrol.h"
#include "padkontrol.h"
#include "virtualmidikontrol.h"
#include "midilistener.h"
#include <iostream>

MainWindow::MainWindow(QWidget *parent) :
    QMainWindow(parent),
    ui(new Ui::MainWindow)
{
    ui->setupUi(this);
    QStateMachine machine;

    MidiKontrol *midiKontrol = new MidiKontrol();
    MidiKontrol *virtualMidi = new MidiKontrol();

    QGroupBox *deviceInGroup = new QGroupBox(tr("Device In"));
    QLabel *deviceInLabel = new QLabel(tr("PadKontrol PORT A"));
    QComboBox *deviceInComboBox = new QComboBox;
    QMapIterator<int, QString> i(midiKontrol->getInputDevicesList());
    while (i.hasNext()) {
         i.next();
         deviceInComboBox->insertItem(i.key(),i.value());
         //std::cout << i.key << i.value() << std::endl;
    }


    QGroupBox *deviceOutGroup = new QGroupBox(tr("Device Out"));
    QLabel *deviceOutLabel = new QLabel(tr("PadKontrol CTRL"));
    QComboBox *deviceOutComboBox = new QComboBox;
    //deviceOutComboBox->addItems(midiKontrol->getOutputDevicesList());
    QMapIterator<int, QString> j(midiKontrol->getOutputDevicesList());
    while (j.hasNext()) {
         j.next();
         deviceOutComboBox->insertItem(j.key(),j.value());
         //std::cout << j.key() << j.value() <<std::endl;
    }

    QGroupBox *midiInGroup = new QGroupBox(tr("Midi In"));
    QLabel *midiInLabel = new QLabel(tr("MidiYoke 1"));
    QComboBox *midiInComboBox = new QComboBox;
    QMapIterator<int, QString> k(virtualMidi->getInputDevicesList());
    while (k.hasNext()) {
         k.next();
         midiInComboBox->insertItem(k.key(),k.value());

    }


    QGroupBox *midiOutGroup = new QGroupBox(tr("Midi Out"));
    QLabel *midiOutLabel = new QLabel(tr("Midi Yoke2"));
    QComboBox *midiOutComboBox = new QComboBox;
    QMapIterator<int, QString> g(virtualMidi->getOutputDevicesList());
    while (g.hasNext()) {
         g.next();
         midiOutComboBox->insertItem(g.key(),g.value());

    }

    QGridLayout *deviceOutLayout = new QGridLayout;
    deviceOutLayout->addWidget(deviceOutLabel, 0, 0);
    deviceOutLayout->addWidget(deviceOutComboBox, 0, 1);
    deviceOutGroup->setLayout(deviceOutLayout);

    QGridLayout *deviceInLayout = new QGridLayout;
    deviceInLayout->addWidget(deviceInLabel, 0, 0);
    deviceInLayout->addWidget(deviceInComboBox, 0, 1);
    deviceInGroup->setLayout(deviceInLayout);

    QGridLayout *midiOutLayout = new QGridLayout;
    midiOutLayout->addWidget(midiOutLabel, 0, 0);
    midiOutLayout->addWidget(midiOutComboBox, 0, 1);
    midiOutGroup->setLayout(midiOutLayout);

    QGridLayout *midiInLayout = new QGridLayout;
    midiInLayout->addWidget(midiInLabel, 0, 0);
    midiInLayout->addWidget(midiInComboBox, 0, 1);
    midiInGroup->setLayout(midiInLayout);


    QPushButton *nativeModeOnBtn = new QPushButton("Enter Native mode", this);
    QPushButton *nativeModeOffBtn = new QPushButton("Exit Native mode", this);

    QGridLayout *layout = new QGridLayout;
    layout->addWidget(deviceOutGroup, 0, 0);
    layout->addWidget(deviceInGroup, 0, 1);
    layout->addWidget(midiOutGroup, 1, 0);
    layout->addWidget(midiInGroup, 1, 1);
    layout->addWidget(nativeModeOnBtn,2,0);
    layout->addWidget(nativeModeOffBtn,2,1);
    centralWidget()->setLayout(layout);

    PadKontrol *padKontrol = new PadKontrol();
    virtualMidiKontrol *vMidiKontrol = new virtualMidiKontrol();

    connect(deviceInComboBox,
            SIGNAL(activated(int)),
            padKontrol,
            SLOT(connectPadIn(int)));
    connect(deviceOutComboBox,
            SIGNAL(activated(int)),
            padKontrol,
            SLOT(connectPadOut(int)));
    connect(midiInComboBox,
            SIGNAL(activated(int)),
            vMidiKontrol,
            SLOT(connectMidiIn(int)));
    connect(midiOutComboBox,
            SIGNAL(activated(int)),
            vMidiKontrol,
            SLOT(connectMidiOut(int)));
    connect(nativeModeOnBtn,
            SIGNAL(clicked()),
            padKontrol,
            SLOT(enterNativeMode()));
    connect(nativeModeOnBtn,
            SIGNAL(clicked()),
            vMidiKontrol,
            SLOT(enterNativeMode()));
    connect(nativeModeOffBtn,
            SIGNAL(clicked()),
            padKontrol,
            SLOT(exitNativeMode()));
    connect(nativeModeOffBtn,
            SIGNAL(clicked()),
            vMidiKontrol,
            SLOT(exitNativeMode()));
    setWindowTitle(tr("totalKontrol"));

    connect(padKontrol,SIGNAL(midiMessage(QString)),vMidiKontrol,SLOT(messageMapper(QString)));
    connect(vMidiKontrol,SIGNAL(midiMessage(QString)),padKontrol,SLOT(messageMapper(QString)));
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


