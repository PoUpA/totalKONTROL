#include "virtualmidikontrol.h"
#include "midikontrol.h"
#include <iostream>
#include <QtXml>

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
        QFile sourceDocument;
        sourceDocument.setFileName("./mappings.xml");
        this->bank="0";

        if (!sourceDocument.open(QIODevice::ReadOnly)) {
            std::cout << "  Couldn't open the file." << std::endl;
            return;
        }

        QXmlStreamReader reader(&sourceDocument);

        this->mappings.clear();
        QStringList mappingElement;
        bool input=false;

        while (!reader.atEnd()) {
            reader.readNext();
            //std::cout << "parsing XML" << std::endl;
            if (reader.isStartElement()) {
                if(reader.name() == "input")
                    input=true;
                if ((reader.name() == "mapping")&&input) {
                    mappingElement.append(reader.attributes().value("bankNumber").toString());
                    mappingElement.append(reader.attributes().value("inputType").toString());
                    mappingElement.append(reader.attributes().value("startWith").toString());
                    mappingElement.append(reader.attributes().value("endWith").toString());
                }
                if(reader.name() == "noteStatus") {
                    mappingElement.append(reader.readElementText());
                }
                else if(reader.name() == "noteData1") {
                    mappingElement.append(reader.readElementText());
                }
                else if(reader.name() == "noteData2") {
                    mappingElement.append(reader.readElementText());
                }
            }
            if(reader.isEndElement()){
                if((reader.name() == "mapping")&&input){
                    this->mappings.append(mappingElement);
                    mappingElement.clear();
                }
                if(reader.name() == "input"){
                    break;
                }
            }


        }

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
    QList<int> mappedMessage;
    mappedMessage.clear();
    //std::cout<< "mapping size :" << this->mappings.size() << std::endl;
    for (int i = 0; i < this->mappings.size(); i++) {
        if (message.startsWith(this->mappings.at(i).at(2))&&message.endsWith(this->mappings.at(i).at(3)) ) {
            if((this->mappings.at(i).at(1)!="switchBank")&&(this->mappings.at(i).at(0)==this->bank)){

                mappedMessage.append(i);
            }
            if((this->mappings.at(i).at(1)=="switchBank")) {
                if(this->mappings.at(i).at(0)!=this->bank){

                    this->bank.clear();
                   this->bank = this->mappings.at(i).at(0);
                   i=0;
                   mappedMessage.clear();

                }
            }


        }
    }
    for (int i = 0; i < mappedMessage.size();++i){
        if(this->mappings.at(mappedMessage.at(i)).at(1)=="button") {
            bool ok;
            midiNote.message=Pm_Message(this->mappings.at(mappedMessage.at(i)).at(4).toUInt(&ok,16), this->mappings.at(mappedMessage.at(i)).at(5).toUInt(&ok,10), this->mappings.at(mappedMessage.at(i)).at(6).toUInt(&ok,10));
            this->virtualMidi->sendMessage(&midiNote);
            for(int j = 7;j < this->mappings.at(mappedMessage.at(i)).count();j=j+3){

                       midiNote.message=Pm_Message(this->mappings.at(mappedMessage.at(i)).at(j).toUInt(&ok,16), this->mappings.at(mappedMessage.at(i)).at(j+1).toUInt(&ok,10), this->mappings.at(mappedMessage.at(i)).at(j+2).toUInt(&ok,10));
                       this->virtualMidi->sendMessage(&midiNote);
            }
        }
        if(this->mappings.at(mappedMessage.at(i)).at(1)=="pad") {
            bool ok;
            QString messageTemp = message.remove(0,14);
            messageTemp = messageTemp.remove(messageTemp.length()-2,2);
            midiNote.message=Pm_Message(this->mappings.at(mappedMessage.at(i)).at(4).toUInt(&ok,16), this->mappings.at(mappedMessage.at(i)).at(5).toUInt(&ok,10), messageTemp.toUInt(&ok,16));
            this->virtualMidi->sendMessage(&midiNote);
        }
        if(this->mappings.at(mappedMessage.at(i)).at(1)=="xy") {
            bool ok;
            QString message_x = message;
            message_x = message_x.remove(0,12);
            message_x = message_x.remove(message_x.length()-4,4);
            QString message_y = message.remove(0,14);
            message_y = message_y.remove(message_y.length()-2,2);
            midiNote.message=Pm_Message(this->mappings.at(mappedMessage.at(i)).at(4).toUInt(&ok,16), this->mappings.at(mappedMessage.at(i)).at(5).toUInt(&ok,10), message_x.toUInt(&ok,16));
            this->virtualMidi->sendMessage(&midiNote);
            midiNote.message=Pm_Message(this->mappings.at(mappedMessage.at(i)).at(7).toUInt(&ok,16), this->mappings.at(mappedMessage.at(i)).at(8).toUInt(&ok,10), message_y.toUInt(&ok,16));
            this->virtualMidi->sendMessage(&midiNote);
        }



    }
    mappedMessage.clear();

}
