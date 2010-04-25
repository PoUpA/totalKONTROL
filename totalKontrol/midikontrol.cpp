#include <QStringList>
#include <QThread>
#include "lib/RtMidi.h"
#include "midikontrol.h"


MidiKontrol::MidiKontrol()
{
    this->inputPort=0;
    this->outputPort=0;
    // RtMidi constructor
    try {
      this->midiin = new RtMidiIn();
      this->midiout = new RtMidiOut();
    }
    catch ( RtError &error ) {
      error.printMessage();
    }
}

MidiKontrol::~MidiKontrol()
{
    delete this->midiin;
    delete this->midiout;
}

QStringList MidiKontrol::getInputDevicesList()
{
    QStringList deviceInputList;
    unsigned int nPorts = this->midiin->getPortCount();
    for(unsigned int i=0;i<nPorts;i++)
    {
        try {
          deviceInputList.append(QString::fromStdString(this->midiin->getPortName(i)));
        }
        catch ( RtError &error ) {
          error.printMessage();

        }
    }
    return deviceInputList;
}

QStringList MidiKontrol::getOutputDevicesList()
{
    QStringList deviceOutputList;

    unsigned int nPorts = this->midiout->getPortCount();
    for(unsigned int i=0;i<nPorts;i++)
    {
        try {
            deviceOutputList.append(QString::fromStdString(this->midiout->getPortName(i)));
        }
        catch ( RtError &error ) {
          error.printMessage();
        }
    }
    return deviceOutputList;
}

void MidiKontrol::setInputPort(unsigned int InputPort)
{
    this->inputPort=InputPort;

}

void MidiKontrol::setOutputPort(unsigned int OutputPort)
{
    this->outputPort=OutputPort;
    this->midiout->openPort(OutputPort);
}
int MidiKontrol::getInputPort()
{
    return this->inputPort;
}
int MidiKontrol::getOutputPort()
{
    return this->outputPort;
}
void MidiKontrol::sendSysEx(QString *message)
{
    std::vector <unsigned char> toSend;
    unsigned int index=0;
    unsigned int temp;
    for(int i=0; i<message->length();i=i+2)
    {
        bool ok;
        temp = message->mid(i,2).toInt(&ok,16);
        toSend.push_back(temp);
        index++;
    }
    this->midiout->sendMessage(&toSend);
}

void MidiKontrol::listenInput()
{
    // Don't ignore sysex, timing, or active sensing messages.
    this->midiin->openPort(this->inputPort);
    this->midiin->ignoreTypes( false, false, false );
    this->midiListener = new MidiListener(this);
    this->midiListener->start();
}

void MidiKontrol::closeInput()
{
    this->midiin->closePort();
    this->midiListener->setListenning(false);
}
