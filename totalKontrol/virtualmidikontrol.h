#ifndef VIRTUALMIDIKONTROL_H
#define VIRTUALMIDIKONTROL_H

#include <QObject>
#include "midikontrol.h"

class virtualMidiKontrol : public QObject
{

    Q_OBJECT

public:
    virtualMidiKontrol();
    virtualMidiKontrol(int padIn, int padOut);
    ~virtualMidiKontrol();

    class MidiKontrol *virtualMidi;
    bool isInit();
public slots:
    void enterNativeMode();
    void exitNativeMode();
    void connectMidiIn(int padIn);
    void connectMidiOut(int padOut);
    void messageMapper(QString message);
signals:
    void midiMessage(QString message);
private:
    bool init;
};

#endif // VIRTUALMIDIKONTROL_H
