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
    //scene led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08011020f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //scene led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08011000f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //message led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08011120f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //message led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08011100f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //setting led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08011220f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //setting led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08011200f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //note/cc led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08011320f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //note/cc led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08011300f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //midi ch led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08011420f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //midi ch led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08011400f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //sw type led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08011520f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //sw type led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08011500f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //relval led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08011620f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //relval led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08011600f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //velocity led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08011720f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //velocity led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08011700f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //port led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08011820f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //port led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08011800f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //fixed_velocity led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08011920f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //fixed_velocity led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08011900f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //prog_change led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08011a20f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //prog_change led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08011a00f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //x led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08011b20f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //x led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08011b00f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //y led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08011c20f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //y led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08011c00f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //knob1_assign led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08011d20f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //knob1_assign led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08011d00f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //knob2_assign led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08011e20f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //knob2_assign led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08011e00f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pedal led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08011f20f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pedal led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08011f00f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //roll led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08012020f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //roll led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08012000f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //flam led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08012120f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //flam led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08012100f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //flam led on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08012220f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //flam led off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08012200f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad1 light on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08010020f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad1 light off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08010000f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad2 light on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08010120f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad2 light off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08010100f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad3 light on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08010220f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad3 light off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08010200f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad4 light on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08010320f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad4 light off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08010300f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad5 light on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08010420f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad5 light off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08010400f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad6 light on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08010520f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad6 light off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08010500f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad7 light on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08010620f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad7 light off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08010600f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad8 light on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08010720f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad8 light off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08010700f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad9 light on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08010820f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad9 light off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08010800f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad10 light on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08010920f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad10 light off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08010900f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad11 light on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08010a20f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad11 light off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08010a00f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad12 light on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08010b20f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad12 light off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08010b00f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad13 light on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08010c20f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad13 light off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08010c00f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad14 light on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08010d20f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad14 light off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08010d00f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad15 light on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08010e20f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad15 light off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08010e00f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad16 light on
    if(message.startsWith("9031")){
        QString sysex ;
        sysex = "f042406e08010f20f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
    //pad16 light off
    else if(message.startsWith("8031")){
        QString sysex ;
        sysex = "f042406e08010f00f7";
        this->padMidiKontrol->sendSysEx(&sysex);
    }
}
