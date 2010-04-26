#include "padkontrol.h"
#include "midikontrol.h"

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
    this->padMidiKontrol->setInputPort(padIn);
}
void PadKontrol::connectPadOut(int padOut)
{
    this->padMidiKontrol->setOutputPort(padOut);
}

void PadKontrol::enterNativeMode()
{
    if(this->init!=true)
    {

        this->padMidiKontrol->listenInput();
        connect(padMidiKontrol->midiListener,SIGNAL(midiMessage(QString)),this,SIGNAL(midiMessage(QString)));
        connect(padMidiKontrol->midiListener,SIGNAL(midiMessage(QString)),this,SLOT(messageMapper(QString)));
        this->init=true;
        QString sysEx;
        sysEx = "F042406E08000001F7F042406E083F2A00000505057F7E7F7F030A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0102030405060708090A0B0C0d0E0F10F7F042406E083F0A01000000000000292929F7";
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
    disconnect(padMidiKontrol->midiListener,SIGNAL(midiMessage(QString)),this,SLOT(messageMapper(QString)));
}

bool PadKontrol::isInit()
{
    return this->init;
}

void PadKontrol::messageMapper(QString message)
{


    if(message == "f042406e84827ff7"){
        QString yesLcd;
        yesLcd = "F042406E083F0A01692C424A0800782D78F7";
        this->padMidiKontrol->sendSysEx(&yesLcd);

        emit btnSettingsOn();
    }
    if(message == "f042406e84807ff7"){
        QString yesLcd;
        yesLcd = "F042406E08220400594553F7";
        this->padMidiKontrol->sendSysEx(&yesLcd);
        emit btnSceneOn();
    }

}
