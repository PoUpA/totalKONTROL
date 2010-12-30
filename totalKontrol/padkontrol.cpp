#include "padkontrol.h"
#include "midikontrol.h"
#include <iostream>
#include <stdlib.h>


PadKontrol::PadKontrol()
{
    this->init=false;
    padMidiKontrol = new MidiKontrol();
}
PadKontrol::PadKontrol(int padIn, int padOut)
{
    this->init=false;
    this->padMidiKontrol = new MidiKontrol();
    this->padMidiKontrol->setInputPort(padIn);
    this->padMidiKontrol->setOutputPort(padOut);
}
PadKontrol::~PadKontrol()
{
    delete this->padMidiKontrol;
}

void PadKontrol::connectPadIn(int padIn)
{
    QMapIterator<int, QString> j(this->padMidiKontrol->getInputDevicesList());
    int index = 0;
    while (j.hasNext()) {
         j.next();
         if(index==padIn){
            this->padMidiKontrol->setInputPort(j.key());
         }
         index++;
         //std::cout << j.key() << j.value() <<std::endl;
    }


}
void PadKontrol::connectPadOut(int padOut)
{
    QMapIterator<int, QString> j(this->padMidiKontrol->getOutputDevicesList());
    int index = 0;
    while (j.hasNext()) {
         j.next();
         if(index==padOut){
            this->padMidiKontrol->setOutputPort(j.key());
         }
         index++;
         //std::cout << j.key() << j.value() <<std::endl;
    }

}

void PadKontrol::enterNativeMode()
{
    if(this->init!=true)
    {

        this->padMidiKontrol->listenInput();
        connect(padMidiKontrol->midiListener,SIGNAL(midiMessage(QString)),this,SIGNAL(midiMessage(QString)));
        //connect(padMidiKontrol->midiListener,SIGNAL(midiMessage(QString)),this,SLOT(messageMapper(QString)));
        this->init=true;
        QString sysEx;
        sysEx = "F042406E08000001F7";
        this->padMidiKontrol->sendSysEx(&sysEx);
        sysEx = "f042406e083f0a01000000000000000000F7";
        this->padMidiKontrol->sendSysEx(&sysEx);
        sysEx = "f042406e0822040150574ef7";
        this->padMidiKontrol->sendSysEx(&sysEx);
        sysEx = "f042406e083f2a00000505057f7e7f7f030a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0102030405060708090a0b0c0d0e0f10f7";
        this->padMidiKontrol->sendSysEx(&sysEx);
        sysEx = "f042406e083f0a010000000000004B5450F7";
        this->padMidiKontrol->sendSysEx(&sysEx);

    }
}
void PadKontrol::exitNativeMode()
{
    QString sysEx;
    sysEx = "F042406E08000000F7";
    this->init=false;
    this->padMidiKontrol->sendSysEx(&sysEx);
    this->padMidiKontrol->closeInput();
    disconnect(padMidiKontrol->midiListener,SIGNAL(midiMessage(QString)),this,SIGNAL(midiMessage(QString)));
    //disconnect(padMidiKontrol->midiListener,SIGNAL(midiMessage(QString)),this,SLOT(messageMapper(QString)));
}

bool PadKontrol::isInit()
{
    return this->init;
}

void PadKontrol::messageMapper(QString message)
{
//TODO : translate midi notes on virtual channel to sysex for output
    //std::cout << message.toStdString() << std::endl;


}
