#include "midilistener.h"
#include "padkontrol.h"

MidiListener::MidiListener(MidiKontrol *midi)
{
    this->midi = midi;
    this->listenning=true;
}
MidiListener::~MidiListener()
{
    this->midi->midiin->closePort();
    this->listenning=false;
}


void MidiListener::run()
{
    QString receviedMessage;
    std::vector <unsigned char> message;
    receviedMessage.clear();
    while(this->listenning)
    {
        this->midi->midiin->getMessage(&message);
        if(message.size()>0)
        {
            for(unsigned int i=0;i<message.size();i++)
            {
                receviedMessage.append(QString::number((int)message[i], 16));
            }
            emit midiMessage(receviedMessage);
            receviedMessage.clear();
        }
        msleep(5);
    }
}

void MidiListener::setListenning(bool listenning)
{
    this->listenning = listenning;
}
