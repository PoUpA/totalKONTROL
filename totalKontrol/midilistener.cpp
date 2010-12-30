#include "midilistener.h"
#include "padkontrol.h"
#include <iostream>
#define MIDI_SYSEX 0xf0
#define MIDI_EOX 0xf7

#define STRING_MAX 80
#define MIDI_MESSAGE_SIZE 1024


MidiListener::MidiListener(MidiKontrol *midi)
{
    this->midi = midi;
    this->listenning=true;
}
MidiListener::~MidiListener()
{
    this->listenning=false;
}


void MidiListener::run()
{
    QString receviedMessage;
    int shift = 0;
    unsigned char data = 0;
    int bytes_on_line = 0;
    PmEvent msg;

    int count;
    receviedMessage.clear();
    while(this->listenning)
    {
        count = Pm_Read(this->midi->midiin, &msg, 1);
        // CAUTION: this causes busy waiting. It would be better to
        // be in a polling loop to avoid being compute bound. PortMidi
        // does not support a blocking read since this is so seldom useful.
        if (count > 0)
        {
            // write 4 bytes of data until you reach an eox
            bool done = 0;

            for ( shift = 0; (shift < 32) && !done; shift += 8) {
                    data = (msg.message >> shift) & 0xFF;
                    // if this is a status byte that's not MIDI_EOX, the sysex
                    // message is incomplete and there is no more sysex data
                    //Convert data to understandable string
                   receviedMessage.append(QString::number((int)data, 16));
                   //if()
                    ++bytes_on_line;
                    done = ( data == MIDI_EOX );
            }
            //Filter sysex
            if(receviedMessage.startsWith("f0")&&done){
                std::cout<< "Sysex done : " << receviedMessage.toStdString() << std::endl;
                emit midiMessage(receviedMessage);
                receviedMessage.clear();
            }
            //Filter non sysex
            if((!receviedMessage.startsWith("f0"))&&(receviedMessage.size()>0))
            {
                std::cout<< "Note done : " << receviedMessage.toStdString() << std::endl;
                emit midiMessage(receviedMessage);
                receviedMessage.clear();
            }
        }
    }
}

void MidiListener::setListenning(bool listenning)
{
    this->listenning = listenning;
}
