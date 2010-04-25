#include "midilistener.h"
#include "padkontrol.h"

MidiListener::MidiListener(class PadKontrol *pad)
{
    this->pad = pad;
}

void MidiListener::run()
{
    bool ok;
    QString receviedMessage;
    std::vector <unsigned char> message;
    while ( true )
    {

        this->pad->padMidiKontrol->midiin->getMessage(&message);
        if(message.size()>0)
        {
            for(unsigned int i=0;i<message.size();i++)
            {
                receviedMessage.append(QString::number(message[i], 16));
            }
            emit midiMessage(receviedMessage);
            receviedMessage.clear();
        }
    }
}
