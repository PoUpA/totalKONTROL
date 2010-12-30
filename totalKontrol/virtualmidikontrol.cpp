#include "virtualmidikontrol.h"
#include "midikontrol.h"
#include <iostream>

virtualMidiKontrol::virtualMidiKontrol()
{
    this->init=false;
    virtualMidi = new MidiKontrol();
}
virtualMidiKontrol::virtualMidiKontrol(int midiIn, int midiOut)
{
    this->init=false;
    this->virtualMidi = new MidiKontrol();
    this->virtualMidi->setInputPort(midiIn);
    this->virtualMidi->setOutputPort(midiOut);
}
virtualMidiKontrol::~virtualMidiKontrol()
{
    delete this->virtualMidi;
}

void virtualMidiKontrol::connectMidiIn(int midiIn)
{
    QMapIterator<int, QString> j(this->virtualMidi->getInputDevicesList());
    int index = 0;
    while (j.hasNext()) {
         j.next();
         if(index==midiIn){
            this->virtualMidi->setInputPort(j.key());
         }
         index++;
         //std::cout << j.key() << j.value() <<std::endl;
    }


}
void virtualMidiKontrol::connectMidiOut(int midiOut)
{
    QMapIterator<int, QString> j(this->virtualMidi->getOutputDevicesList());
    int index = 0;
    while (j.hasNext()) {
         j.next();
         if(index==midiOut){
            this->virtualMidi->setOutputPort(j.key());
         }
         index++;
         //std::cout << j.key() << j.value() <<std::endl;
    }

}
void virtualMidiKontrol::enterNativeMode()
{
    if(this->init!=true)
    {
        this->virtualMidi->listenInput();
        connect(virtualMidi->midiListener,SIGNAL(midiMessage(QString)),this,SIGNAL(midiMessage(QString)));

    }
}
void virtualMidiKontrol::exitNativeMode()
{
    this->init=false;
    this->virtualMidi->closeInput();
    disconnect(virtualMidi->midiListener,SIGNAL(midiMessage(QString)),this,SIGNAL(midiMessage(QString)));
    //disconnect(virtualMidi->midiListener,SIGNAL(midiMessage(QString)),this,SLOT(messageMapper(QString)));
}


bool virtualMidiKontrol::isInit()
{
    return this->init;
}

void virtualMidiKontrol::messageMapper(QString message)
{
    //TODO : translate sysex inputs to midi notes

    //Settings Button On
    if(message == "f042406e84827ff7"){
        PmEvent midiNote;
        midiNote.timestamp=0;
        midiNote.message=Pm_Message(0x90, 1, 100);
        this->virtualMidi->sendMessage(&midiNote);
    }
    //Settings Button Off
    if(message == "f042406e84820f7"){
        PmEvent midiNote;
        midiNote.timestamp=0;
        midiNote.message=Pm_Message(0x80, 1, 0);
        this->virtualMidi->sendMessage(&midiNote);
    }
    //Encoder left
    if(message == "f042406e84307ff7"){
        PmEvent midiNote;
        midiNote.timestamp=0;
        midiNote.message=Pm_Message(0x90, 2, 100);
        this->virtualMidi->sendMessage(&midiNote);
        midiNote.timestamp=0;
        midiNote.message=Pm_Message(0x80, 2, 0);
        this->virtualMidi->sendMessage(&midiNote);
    }
    //Encoder right
    if(message == "f042406e84301f7"){
        PmEvent midiNote;
        midiNote.timestamp=0;
        midiNote.message=Pm_Message(0x90, 3, 100);
        this->virtualMidi->sendMessage(&midiNote);
        midiNote.timestamp=0;
        midiNote.message=Pm_Message(0x80, 3, 0);
        this->virtualMidi->sendMessage(&midiNote);
    }


}
