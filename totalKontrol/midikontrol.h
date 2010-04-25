#ifndef MIDIKONTROL_H
#define MIDIKONTROL_H

#include <QObject>
#include "lib/RtMidi.h"
#include "midilistener.h"
#include "padkontrol.h"

class MidiKontrol : public QObject
{
    Q_OBJECT
public:
    MidiKontrol();
    ~MidiKontrol();

    QStringList getInputDevicesList();
    QStringList getOutputDevicesList();
    class MidiListener *midiListener;
    RtMidiIn  *midiin;
    RtMidiOut *midiout;
    void setPadKontrolInputPort(unsigned int InputPort);
    void setPadKontrolOutputPort(unsigned int OutputPort);
    void listenInput(class PadKontrol *pad);

    void sendSysEx(QString *message);


private:


    unsigned int padKontrolInputPort;
    unsigned int padKontrolOutputPort;
};

#endif // MIDIKONTROL_H
