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
    ~PadKontrol();

    class MidiKontrol *padMidiKontrol;
    class QList<QStringList> mappings;
    bool isInit();
public slots:
    void enterNativeMode();
    void exitNativeMode();
    void connectPadIn(int padIn);
    void connectPadOut(int padOut);
    void messageMapper(QString message);
signals:
    void midiMessage(QString message);
    /* One signal for each buttons */
    void rotaryLeft();
    void rotaryRight();
    void btnSettingsOn();
    void btnSettingsOff();
    void btnXpadOn();
    void btnXpadOff();
    void btnYpadOn();
    void btnYpadOff();
    void btnPedalOn();
    void btnPedalOff();
    void btnNoteCCOn();
    void btnNoteCCOff();
    void btnMidiCHOn();
    void btnMidiCHOff();
    void btnSWTypeOn();
    void btnSWTypeOff();
    void btnRelValOn();
    void btnRelValOff();
    void btnVelocityOn();
    void btnVelocityOff();
    void btnPortOn();
    void btnPortOff();
    void btnSceneOn();
    void btnSceneOff();
    void btnMessageOn();
    void btnMessageOff();
    void btnFixedVelocityOn();
    void btnFixedVelocityOff();
    void btnProgChangeOn();
    void btnProgChangeOff();
    void btnHoldOn();
    void btnHoldOff();
    void btnFlamOn();
    void btnFlamOff();
    void btnRollOn();
    void btnRollOff();

    void pad01On(int velocity);
    void pad02On(int velocity);
    void pad03On(int velocity);
    void pad04On(int velocity);
    void pad05On(int velocity);
    void pad06On(int velocity);
    void pad07On(int velocity);
    void pad08On(int velocity);
    void pad09On(int velocity);
    void pad10On(int velocity);
    void pad11On(int velocity);
    void pad12On(int velocity);
    void pad13On(int velocity);
    void pad14On(int velocity);
    void pad15On(int velocity);
    void pad16On(int velocity);

    void pad01Off();
    void pad02Off();
    void pad03Off();
    void pad04Off();
    void pad05Off();
    void pad06Off();
    void pad07Off();
    void pad08Off();
    void pad09Off();
    void pad10Off();
    void pad11Off();
    void pad12Off();
    void pad13Off();
    void pad14Off();
    void pad15Off();
    void pad16Off();
private:
    bool init;

};

#endif // PADKONTROL_H
