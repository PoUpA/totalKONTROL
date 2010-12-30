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
    mainwindow.cpp \
    padkontrol.cpp \
    midikontrol.cpp \
    midilistener.cpp \
    lib/pmwinmm.c \
    lib/pmwin.c \
    lib/ptwinmm.c \
    lib/porttime.c \
    lib/portmidi.c \
    lib/pmutil.c \
    virtualmidikontrol.cpp
HEADERS += mainwindow.h \
    padkontrol.h \
    midikontrol.h \
    midilistener.h \
    lib/pmwinmm.h \
    lib/porttime.h \
    lib/portmidi.h \
    lib/pmutil.h \
    lib/pminternal.h \
    virtualmidikontrol.h
LIBS += -lwinmm -advapi32
DEFINES -= UNICODE
FORMS += mainwindow.ui

OTHER_FILES += \
    lib/porttime.dsp
