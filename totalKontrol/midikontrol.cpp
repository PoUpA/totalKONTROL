#include <QString>
#include <QStringList>
#include <QThread>
#include <iostream>
#include <vector>
#include <cstdlib>
#include "midikontrol.h"
#include "lib/portmidi.h"

#define MIDI_SYSEX 0xf0
#define MIDI_EOX 0xf7

#define STRING_MAX 80
#define MIDI_MESSAGE_SIZE 1024



#define INPUT_BUFFER_SIZE 100
#define OUTPUT_BUFFER_SIZE 0
#define DRIVER_INFO NULL
#define TIME_PROC PmTimestamp()
#define TIME_INFO NULL
#define TIME_START Pt_Start(1, 0, 0) /* timer started w/millisecond accuracy */

MidiKontrol::MidiKontrol()
{
    this->inputPort=0;
    this->outputPort=0;
    this->midiin=NULL;
    this->midiout=NULL;
}

MidiKontrol::~MidiKontrol()
{
    delete this->midiin;
    delete this->midiout;
}

QMap<int, QString> MidiKontrol::getInputDevicesList()
{
    QMap<int, QString> deviceInputList;
    int iNumDevices = Pm_CountDevices();
    const PmDeviceInfo *deviceInfo;

    for (int i = 0; i < iNumDevices; i++)
    {
        deviceInfo = Pm_GetDeviceInfo(i);
        //std::cout << "#"<< i << deviceInfo->name <<std::endl;
        if (deviceInfo->input) {
            deviceInputList[i]=deviceInfo->name;
        }
    }
    return deviceInputList;
}

QMap<int, QString> MidiKontrol::getOutputDevicesList()
{
    QMap<int, QString> deviceOutputList;
    unsigned int nPorts = Pm_CountDevices();
    const PmDeviceInfo *deviceInfo;
    for(unsigned int i=0;i<nPorts;i++)
    {
        deviceInfo = Pm_GetDeviceInfo(i);
        if (deviceInfo->output) {
            deviceOutputList[i]=deviceInfo->name;
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
}
int MidiKontrol::getInputPort()
{
    return this->inputPort;
}
int MidiKontrol::getOutputPort()
{
    return this->outputPort;
}
void MidiKontrol::sendSysEx(QString *message, int timer)
{
    PmError OutputError;
    unsigned char * str;
    str = new unsigned char [message->length()];
    OutputError = Pm_OpenOutput(&this->midiout,(PmDeviceID)this->outputPort, DRIVER_INFO, OUTPUT_BUFFER_SIZE, NULL, NULL, 0);
    if( OutputError != pmNoError )
    {
        std::cout << "PortMidi error:" << Pm_GetErrorText(OutputError);

    }
    unsigned int index=0;
    for(int i=0; i<message->length();i=i+2)
    {
        bool ok;
        str[index] = message->mid(i,2).toInt(&ok,16);
        index++;
    }
    OutputError = Pm_WriteSysEx(this->midiout,timer,str);
    if( OutputError != pmNoError ) std::cout << "PortMidi output #"<<this->outputPort<<" sendSysExMsg error:" << Pm_GetErrorText(OutputError);
    Pm_Close(this->midiout);
}

void MidiKontrol::sendMessage(PmEvent *message)
{
    PmError OutputError;
    //unsigned char * str;
    //str = new unsigned char [message->length()];
    OutputError = Pm_OpenOutput(&this->midiout,(PmDeviceID)this->outputPort, DRIVER_INFO, OUTPUT_BUFFER_SIZE, NULL, NULL, 0);
    if( OutputError != pmNoError )
    {
        std::cout << "PortMidi error:" << Pm_GetErrorText(OutputError);

    }

    OutputError = Pm_Write(this->midiout,message,1);
    if( OutputError != pmNoError ) std::cout << "PortMidi sendMessage error:" << Pm_GetErrorText(OutputError);
    Pm_Close(this->midiout);
}

void MidiKontrol::listenInput()
{
    PmError InputError;
    InputError = Pm_OpenInput(&this->midiin,(PmDeviceID)this->inputPort, DRIVER_INFO, INPUT_BUFFER_SIZE, TIME_PROC, TIME_INFO);
    if( InputError != pmNoError )
    {
        std::cout << "PortMidi Open Input #"<<this->inputPort<<" error:"  << Pm_GetErrorText(InputError);

    }
    this->midiListener = new MidiListener(this);
    this->midiListener->start();
}

void MidiKontrol::closeInput()
{
    this->midiListener->setListenning(false);
    Pm_Close(this->midiin);

}
