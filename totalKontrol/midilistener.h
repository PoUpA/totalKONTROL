#ifndef MIDILISTENER_H
#define MIDILISTENER_H

#include <QThread>
#include "midikontrol.h"

class MidiListener : public QThread
{
    Q_OBJECT

public:
    MidiListener(class MidiKontrol *midi);
    ~MidiListener();
    class MidiKontrol *midi;
    virtual void run();
    void setListenning(bool listenning);
    bool listenning;
signals:
    void midiMessage(QString message);
    void midiMessage(PmEvent message);
    
};

#endif // MIDILISTENER_H
