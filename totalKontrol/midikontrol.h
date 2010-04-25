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
    void setInputPort(unsigned int InputPort);
    void setOutputPort(unsigned int OutputPort);
    int getInputPort();
    int getOutputPort();

    void listenInput();
    void closeInput();

    void sendSysEx(QString *message);


private:


    unsigned int inputPort;
    unsigned int outputPort;
};

#endif // MIDIKONTROL_H
