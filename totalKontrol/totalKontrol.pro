# -------------------------------------------------
# Project created by QtCreator 2010-04-15T01:20:04
# -------------------------------------------------
QT += script \
    xml
TARGET = totalKontrol
CONFIG += console
CONFIG -= app_bundle
TEMPLATE = app
SOURCES += main.cpp \
    lib/RtMidi.cpp \
    mainwindow.cpp \
    padkontrol.cpp \
    midikontrol.cpp \
    midilistener.cpp
HEADERS += lib/RtMidi.h \
    lib/RtError.h \
    mainwindow.h \
    padkontrol.h \
    midikontrol.h \
    midilistener.h
LIBS += -lwinmm
FORMS += mainwindow.ui
