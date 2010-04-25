#ifndef PADKONTROL_H
#define PADKONTROL_H

#include <QObject>
#include "midikontrol.h"

class PadKontrol : public QObject
{

    Q_OBJECT

public:
    PadKontrol();
    PadKontrol(int padIn, int padOut);

    class MidiKontrol *padMidiKontrol;
    bool isInit();
public slots:
    void enterNativeMode();
    void exitNativeMode();
    void connectPadIn(int padIn);
    void connectPadOut(int padOut);
    void messageMapper(QString message);
signals:
    void midiMessage(QString message);
private:
    bool init;

};

#endif // PADKONTROL_H
