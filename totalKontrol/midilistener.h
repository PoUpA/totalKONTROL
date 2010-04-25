#ifndef MIDILISTENER_H
#define MIDILISTENER_H

#include <QThread>
#include "padkontrol.h"

class MidiListener : public QThread
{
    Q_OBJECT

public:
    MidiListener(class PadKontrol *pad);
    class PadKontrol *pad;
    virtual void run();
signals:
    void midiMessage(QString message);

};

#endif // MIDILISTENER_H
