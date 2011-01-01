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
    if(message == "f042406e0848027ff7"){
        midiNote.message=Pm_Message(0x90, 1, 100);
    }
    //Settings Button Off
    else if(message == "f042406e08480200f7"){
        midiNote.message=Pm_Message(0x80, 1, 0);
    }
    //Encoder left
    else if(message == "f042406e0843007ff7"){
        midiNote.message=Pm_Message(0x90, 2, 100);
        this->virtualMidi->sendMessage(&midiNote);
        midiNote.message=Pm_Message(0x80, 2, 0);
    }
    //Encoder right
    else if(message == "f042406e08430001f7"){
        midiNote.message=Pm_Message(0x90, 3, 100);
        this->virtualMidi->sendMessage(&midiNote);
        midiNote.message=Pm_Message(0x80, 3, 0);
    }
    //button x On
    else if(message == "f042406e08480b7ff7"){
        midiNote.message=Pm_Message(0x90, 4, 100);
    }
    //button x Off
    else if(message == "f042406e08480b00f7"){
        midiNote.message=Pm_Message(0x80, 4, 0);
    }
    //button y On
    else if(message == "f042406e08480c7ff7"){
        midiNote.message=Pm_Message(0x90, 5, 100);
    }
    //button y Off
    else if(message == "f042406e08480c00f7"){
        midiNote.message=Pm_Message(0x80, 5, 0);
    }
    //button pedal On
    else if(message == "f042406e08480f7ff7"){
        midiNote.message=Pm_Message(0x90, 6, 100);
    }
    //button pedal Off
    else if(message == "f042406e08480f00f7"){
        midiNote.message=Pm_Message(0x80, 6, 0);
    }
    //button note On
    else if(message == "f042406e0848037ff7"){
        midiNote.message=Pm_Message(0x90, 7, 100);
    }
    //button note Off
    else if(message == "f042406e08480300f7"){
        midiNote.message=Pm_Message(0x80, 7, 0);
    }
    //button midi_ch On
    else if(message == "f042406e0848047ff7"){
        midiNote.message=Pm_Message(0x90, 8, 100);
    }
    //button midi_ch Off
    else if(message == "f042406e08480400f7"){
        midiNote.message=Pm_Message(0x80, 8, 0);
    }
    //button sw_type On
    else if(message == "f042406e0848057ff7"){
        midiNote.message=Pm_Message(0x90, 9, 100);
    }
    //button sw_type Off
    else if(message == "f042406e08480500f7"){
        midiNote.message=Pm_Message(0x80, 9, 0);
    }
    //button rel_val On
    else if(message == "f042406e0848067ff7"){
        midiNote.message=Pm_Message(0x90, 10, 100);
    }
    //button rel_val Off
    else if(message == "f042406e08480600f7"){
        midiNote.message=Pm_Message(0x80, 10, 0);
    }
    //button velocity On
    else if(message == "f042406e0848077ff7"){
        midiNote.message=Pm_Message(0x90, 11, 100);
    }
    //button velocity Off
    else if(message == "f042406e08480700f7"){
        midiNote.message=Pm_Message(0x80, 11, 0);
    }
    //button port On
    else if(message == "f042406e0848087ff7"){
        midiNote.message=Pm_Message(0x90, 12, 100);
    }
    //button port Off
    else if(message == "f042406e08480800f7"){
        midiNote.message=Pm_Message(0x80, 12, 0);
    }
    //button scene On
    else if(message == "f042406e0848007ff7"){
        midiNote.message=Pm_Message(0x90, 13, 100);
    }
    //button scene Off
    else if(message == "f042406e08480000f7"){
        midiNote.message=Pm_Message(0x80, 13, 0);
    }
    //button message On
    else if(message == "f042406e0848017ff7"){
        midiNote.message=Pm_Message(0x90, 14, 100);
    }
    //button message Off
    else if(message == "f042406e08480100f7"){
        midiNote.message=Pm_Message(0x80, 14, 0);
    }
    //button fixed_velocity On
    else if(message == "f042406e0848097ff7"){
        midiNote.message=Pm_Message(0x90, 15, 100);
    }
    //button fixed_velocity Off
    else if(message == "f042406e08480900f7"){
        midiNote.message=Pm_Message(0x80, 15, 0);
    }
    //button prog_change On
    else if(message == "f042406e08480a7ff7"){
        midiNote.message=Pm_Message(0x90, 16, 100);
    }
    //button prog_change Off
    else if(message == "f042406e08480a00f7"){
        midiNote.message=Pm_Message(0x80, 16, 0);
    }
    //button hold On
    else if(message == "f042406e0848127ff7"){
        midiNote.message=Pm_Message(0x90, 17, 100);
    }
    //button hold Off
    else if(message == "f042406e08481200f7"){
        midiNote.message=Pm_Message(0x80, 17, 0);
    }
    //button flam On
    else if(message == "f042406e0848117ff7"){
        midiNote.message=Pm_Message(0x90, 18, 100);
    }
    //button flam Off
    else if(message == "f042406e08481100f7"){
        midiNote.message=Pm_Message(0x80, 18, 0);
    }
    //button roll On
    else if(message == "f042406e0848107ff7"){
        midiNote.message=Pm_Message(0x90, 19, 100);
    }
    //button roll Off
    else if(message == "f042406e08481000f7"){
        midiNote.message=Pm_Message(0x80, 19, 0);
    }
    //button knob1_assign On
    else if(message == "f042406e08480d7ff7"){
        midiNote.message=Pm_Message(0x90, 20, 100);
    }
    //button knob1_assign Off
    else if(message == "f042406e08480d00f7"){
        midiNote.message=Pm_Message(0x80, 20, 0);
    }
    //button knob2_assign On
    else if(message == "f042406e08480e7ff7"){
        midiNote.message=Pm_Message(0x90, 21, 100);
    }
    //button knob2_assign Off
    else if(message == "f042406e08480e00f7"){
        midiNote.message=Pm_Message(0x80, 21, 0);
    }
    //button pad1 On
    else if(message.startsWith("f042406e084540")&&message.endsWith("f7")){
        bool ok;
        message = message.remove(0,14);
        message = message.remove(message.length()-2,2);
        midiNote.message=Pm_Message(0x90, 22, message.toUInt(&ok,16));
    }
    //button pad1 Off
    else if(message == "f042406e08450040f7"){
        midiNote.message=Pm_Message(0x80, 22, 0);
    }
    //button pad2 On
    else if(message.startsWith("f042406e084541")&&message.endsWith("f7")){
        bool ok;
        message = message.remove(0,14);
        message = message.remove(message.length()-2,2);
        midiNote.message=Pm_Message(0x90, 23, message.toUInt(&ok,16));
    }
    //button pad2 Off
    else if(message == "f042406e08450140f7"){
        midiNote.message=Pm_Message(0x80, 23, 0);
    }
    //button pad3 On
    else if(message.startsWith("f042406e084542")&&message.endsWith("f7")){
        bool ok;
        message = message.remove(0,14);
        message = message.remove(message.length()-2,2);
        midiNote.message=Pm_Message(0x90, 24, message.toUInt(&ok,16));
    }
    //button pad3 Off
    else if(message == "f042406e08450240f7"){
        midiNote.message=Pm_Message(0x80, 24, 0);
    }
    //button pad4 On
    else if(message.startsWith("f042406e084543")&&message.endsWith("f7")){
        bool ok;
        message = message.remove(0,14);
        message = message.remove(message.length()-2,2);
        midiNote.message=Pm_Message(0x90, 25, message.toUInt(&ok,16));

    }
    //button pad4 Off
    else if(message == "f042406e08450340f7"){
        midiNote.message=Pm_Message(0x80, 25, 0);
    }
    //button pad5 On
    else if(message.startsWith("f042406e084544")&&message.endsWith("f7")){
        bool ok;
        message = message.remove(0,14);
        message = message.remove(message.length()-2,2);
        midiNote.message=Pm_Message(0x90, 26, message.toUInt(&ok,16));
    }
    //button pad5 Off
    else if(message == "f042406e08450440f7"){
        midiNote.message=Pm_Message(0x80, 26, 0);
    }
    //button pad6 On
    else if(message.startsWith("f042406e084545")&&message.endsWith("f7")){
        bool ok;
        message = message.remove(0,14);
        message = message.remove(message.length()-2,2);
        midiNote.message=Pm_Message(0x90, 27, message.toUInt(&ok,16));
    }
    //button pad6 Off
    else if(message == "f042406e08450540f7"){
        midiNote.message=Pm_Message(0x80, 27, 0);
    }
    //button pad7 On
    else if(message.startsWith("f042406e084546")&&message.endsWith("f7")){
        bool ok;
        message = message.remove(0,14);
        message = message.remove(message.length()-2,2);
        midiNote.message=Pm_Message(0x90, 28, message.toUInt(&ok,16));
    }
    //button pad7 Off
    else if(message == "f042406e08450640f7"){
        midiNote.message=Pm_Message(0x80, 28, 0);
    }
    //button pad8 On
    else if(message.startsWith("f042406e084547")&&message.endsWith("f7")){
        bool ok;
        message = message.remove(0,14);
        message = message.remove(message.length()-2,2);
        midiNote.message=Pm_Message(0x90, 29, message.toUInt(&ok,16));

    }
    //button pad8 Off
    else if(message == "f042406e08450740f7"){
        midiNote.message=Pm_Message(0x80, 29, 0);
    }
    //button pad9 On
    else if(message.startsWith("f042406e084548")&&message.endsWith("f7")){
        bool ok;
        message = message.remove(0,14);
        message = message.remove(message.length()-2,2);
        midiNote.message=Pm_Message(0x90, 30, message.toUInt(&ok,16));
    }
    //button pad9 Off
    else if(message == "f042406e08450840f7"){
        midiNote.message=Pm_Message(0x80, 30, 0);
    }
    //button pad10 On
    else if(message.startsWith("f042406e084549")&&message.endsWith("f7")){
        bool ok;
        message = message.remove(0,14);
        message = message.remove(message.length()-2,2);
        midiNote.message=Pm_Message(0x90, 31, message.toUInt(&ok,16));
    }
    //button pad10 Off
    else if(message == "f042406e08450940f7"){
        midiNote.message=Pm_Message(0x80, 31, 0);
    }
    //button pad11 On
    else if(message.startsWith("f042406e08454a")&&message.endsWith("f7")){
        bool ok;
        message = message.remove(0,14);
       message = message.remove(message.length()-2,2);
        midiNote.message=Pm_Message(0x90, 32, message.toUInt(&ok,16));
    }
    //button pad11 Off
    else if(message == "f042406e08450a40f7"){
        midiNote.message=Pm_Message(0x80, 32, 0);
    }
    //button pad12 On
    else if(message.startsWith("f042406e08454b")&&message.endsWith("f7")){
        bool ok;
        message = message.remove(0,14);
        message = message.remove(message.length()-2,2);
        midiNote.message=Pm_Message(0x90, 33, message.toUInt(&ok,16));
    }
    //button pad12 Off
    else if(message == "f042406e08450b40f7"){
        midiNote.message=Pm_Message(0x80, 33, 0);
    }
    //button pad13 On
    else if(message.startsWith("f042406e08454c")&&message.endsWith("f7")){
        bool ok;
        message = message.remove(0,14);
        message = message.remove(message.length()-2,2);
        midiNote.message=Pm_Message(0x90, 33, message.toUInt(&ok,16));
    }
    //button pad13 Off
    else if(message == "f042406e08450c40f7"){
        midiNote.message=Pm_Message(0x80, 33, 0);
    }
    //button pad14 On
    else if(message.startsWith("f042406e08454d")&&message.endsWith("f7")){
        bool ok;
        message = message.remove(0,14);
        message = message.remove(message.length()-2,2);
        midiNote.message=Pm_Message(0x90, 34, message.toUInt(&ok,16));

    }
    //button pad14 Off
    else if(message == "f042406e08450d40f7"){
        midiNote.message=Pm_Message(0x80, 34, 0);
    }
    //button pad15 On
    else if(message.startsWith("f042406e08454e")&&message.endsWith("f7")){
        bool ok;
        message = message.remove(0,14);
        message = message.remove(message.length()-2,2);
        midiNote.message=Pm_Message(0x90, 35, message.toUInt(&ok,16));
    }
    //button pad15 Off
    else if(message == "f042406e08450e40f7"){
        midiNote.message=Pm_Message(0x80, 35, 0);
    }
    //button pad16 On
    else if(message.startsWith("f042406e08454f")&&message.endsWith("f7")){
        bool ok;
        message = message.remove(0,14);
        message = message.remove(message.length()-2,2);
        midiNote.message=Pm_Message(0x90, 36, message.toUInt(&ok,16));

    }
    //button pad16 Off
    else if(message == "f042406e08450f40f7"){
        midiNote.message=Pm_Message(0x80, 36, 0);
    }
    //button knob1 value
    else if(message.startsWith("f042406e084900")&&message.endsWith("f7")){
        bool ok;
        message = message.remove(0,14);
        message = message.remove(message.length()-2,2);
        midiNote.message=Pm_Message(0xb0, 1, message.toUInt(&ok,16));
    }
    //button knob2 value
    else if(message.startsWith("f042406e084901")&&message.endsWith("f7")){
        bool ok;
        message = message.remove(0,14);
        message = message.remove(message.length()-2,2);
        midiNote.message=Pm_Message(0xb0, 2, message.toUInt(&ok,16));
    }
    //button ypad value
    else if(message.startsWith("f042406e084b")&&message.endsWith("f7")){
        bool ok;
        QString message_x = message;
        message_x = message_x.remove(0,12);
        message_x = message_x.remove(message_x.length()-4,4);
        QString message_y = message.remove(0,14);
        message_y = message_y.remove(message_y.length()-2,2);
        midiNote.message=Pm_Message(0xb0, 3, message_x.toUInt(&ok,16));
        this->virtualMidi->sendMessage(&midiNote);
        midiNote.message=Pm_Message(0xb0, 4, message_y.toUInt(&ok,16));

    }
    else {
        return;
    }
    this->virtualMidi->sendMessage(&midiNote);
}
