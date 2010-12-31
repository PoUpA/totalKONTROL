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
    PmEvent midiNote;
    midiNote.timestamp=0;
    //Settings Button On
    if(message == "f042406e84827ff7"){
        midiNote.message=Pm_Message(0x90, 1, 100);
    }
    //Settings Button Off
    else if(message == "f042406e84820f7"){
        midiNote.message=Pm_Message(0x80, 1, 0);
    }
    //Encoder left
    else if(message == "f042406e84307ff7"){
        midiNote.message=Pm_Message(0x90, 2, 100);
        this->virtualMidi->sendMessage(&midiNote);
        midiNote.message=Pm_Message(0x80, 2, 0);
    }
    //Encoder right
    else if(message == "f042406e84301f7"){
        midiNote.message=Pm_Message(0x90, 3, 100);
        this->virtualMidi->sendMessage(&midiNote);
        midiNote.message=Pm_Message(0x80, 3, 0);
    }
    //button x On
    else if(message == "f042406e848b7ff7"){
        midiNote.message=Pm_Message(0x90, 4, 100);
    }
    //button x Off
    else if(message == "f042406e848b0f7"){
        midiNote.message=Pm_Message(0x80, 4, 0);
    }
    //button y On
    else if(message == "f042406e848c7ff7"){
        midiNote.message=Pm_Message(0x90, 5, 100);
    }
    //button y Off
    else if(message == "f042406e848c0f7"){
        midiNote.message=Pm_Message(0x80, 5, 0);
    }
    //button pedal On
    else if(message == "f042406e848f7ff7"){
        midiNote.message=Pm_Message(0x90, 6, 100);
    }
    //button pedal Off
    else if(message == "f042406e848f0f7"){
        midiNote.message=Pm_Message(0x80, 6, 0);
    }
    //button note On
    else if(message == "f042406e84837ff7"){
        midiNote.message=Pm_Message(0x90, 7, 100);
    }
    //button note Off
    else if(message == "f042406e84830f7"){
        midiNote.message=Pm_Message(0x80, 7, 0);
    }
    //button midi_ch On
    else if(message == "f042406e84847ff7"){
        midiNote.message=Pm_Message(0x90, 8, 100);
    }
    //button midi_ch Off
    else if(message == "f042406e84840f7"){
        midiNote.message=Pm_Message(0x80, 8, 0);
    }
    //button sw_type On
    else if(message == "f042406e84857ff7"){
        midiNote.message=Pm_Message(0x90, 9, 100);
    }
    //button sw_type Off
    else if(message == "f042406e84850f7"){
        midiNote.message=Pm_Message(0x80, 9, 0);
    }
    //button rel_val On
    else if(message == "f042406e84867ff7"){
        midiNote.message=Pm_Message(0x90, 10, 100);
    }
    //button rel_val Off
    else if(message == "f042406e84860f7"){
        midiNote.message=Pm_Message(0x80, 10, 0);
    }
    //button velocity On
    else if(message == "f042406e84877ff7"){
        midiNote.message=Pm_Message(0x90, 11, 100);
    }
    //button velocity Off
    else if(message == "f042406e84870f7"){
        midiNote.message=Pm_Message(0x80, 11, 0);
    }
    //button port On
    else if(message == "f042406e84887ff7"){
        midiNote.message=Pm_Message(0x90, 12, 100);
    }
    //button port Off
    else if(message == "f042406e84880f7"){
        midiNote.message=Pm_Message(0x80, 12, 0);
    }
    //button scene On
    else if(message == "f042406e84807ff7"){
        midiNote.message=Pm_Message(0x90, 13, 100);
    }
    //button scene Off
    else if(message == "f042406e84800f7"){
        midiNote.message=Pm_Message(0x80, 13, 0);
    }
    //button message On
    else if(message == "f042406e84817ff7"){
        midiNote.message=Pm_Message(0x90, 14, 100);
    }
    //button message Off
    else if(message == "f042406e84810f7"){
        midiNote.message=Pm_Message(0x80, 14, 0);
    }
    //button fixed_velocity On
    else if(message == "f042406e84897ff7"){
        midiNote.message=Pm_Message(0x90, 15, 100);
    }
    //button fixed_velocity Off
    else if(message == "f042406e84890f7"){
        midiNote.message=Pm_Message(0x80, 15, 0);
    }
    //button prog_change On
    else if(message == "f042406e848a7ff7"){
        midiNote.message=Pm_Message(0x90, 16, 100);
    }
    //button prog_change Off
    else if(message == "f042406e848a0f7"){
        midiNote.message=Pm_Message(0x80, 16, 0);
    }
    //button hold On
    else if(message == "f042406e848127ff7"){
        midiNote.message=Pm_Message(0x90, 17, 100);
    }
    //button hold Off
    else if(message == "f042406e848120f7"){
        midiNote.message=Pm_Message(0x80, 17, 0);
    }
    //button flam On
    else if(message == "f042406e848117ff7"){
        midiNote.message=Pm_Message(0x90, 18, 100);
    }
    //button flam Off
    else if(message == "f042406e848110f7"){
        midiNote.message=Pm_Message(0x80, 18, 0);
    }
    //button roll On
    else if(message == "f042406e848107ff7"){
        midiNote.message=Pm_Message(0x90, 19, 100);
    }
    //button roll Off
    else if(message == "f042406e848100f7"){
        midiNote.message=Pm_Message(0x80, 19, 0);
    }
    //button knob1_assign On
    else if(message == "f042406e848d7ff7"){
        midiNote.message=Pm_Message(0x90, 20, 100);
    }
    //button knob1_assign Off
    else if(message == "f042406e848d0f7"){
        midiNote.message=Pm_Message(0x80, 20, 0);
    }
    //button knob2_assign On
    else if(message == "f042406e848e7ff7"){
        midiNote.message=Pm_Message(0x90, 21, 100);
    }
    //button knob2_assign Off
    else if(message == "f042406e848e0f7"){
        midiNote.message=Pm_Message(0x80, 21, 0);
    }
    else {
        return;
    }
    this->virtualMidi->sendMessage(&midiNote);
}
