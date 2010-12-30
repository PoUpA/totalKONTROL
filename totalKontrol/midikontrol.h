#ifndef MIDIKONTROL_H
#define MIDIKONTROL_H
#include "lib/portmidi.h"
#include "lib/porttime.h"
#include <QObject>
#include "midilistener.h"
#include "padkontrol.h"
#include <QMap>

class MidiKontrol : public QObject
{
    Q_OBJECT
public:
    MidiKontrol();
    ~MidiKontrol();

    QMap<int, QString> getInputDevicesList();
    QMap<int, QString> getOutputDevicesList();
    class MidiListener *midiListener;
    PortMidiStream  *midiin;
    PortMidiStream  *midiout;
    void setInputPort(unsigned int InputPort);
    void setOutputPort(unsigned int OutputPort);
    int getInputPort();
    int getOutputPort();

    void listenInput();
    void closeInput();

    void sendSysEx(QString *message, int time=0);
    void sendMessage(PmEvent *message);


private:


    unsigned int inputPort;
    unsigned int outputPort;
};

#endif // MIDIKONTROL_H
