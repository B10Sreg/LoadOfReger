import QtQuick 2.15
import SddmComponents 2.0

Rectangle {
    id: root
    width: 1376
    height: 768
    color: config.bgBase

    LayoutMirroring.enabled: Qt.locale().textDirection == Qt.RightToLeft
    LayoutMirroring.childrenInherit: true

    property int sessionIndex: session.index
    property color bgBase: config.bgBase
    property color bgAlt: config.bgAlt
    property color fgBase: config.fgBase
    property color fgAccent: config.fgAccent
    property color borderColor: config.borderColor
    property color placeholderColor: config.placeholderColor
    property string fontFamily: config.fontFamily

    TextConstants { id: textConstants }

    Connections {
        target: sddm
        onLoginSucceeded: {
            statusText.color = root.fgAccent
            statusText.text = textConstants.loginSucceeded
        }
        onLoginFailed: {
            password.text = ""
            statusText.color = config.failureColor
            statusText.text = textConstants.loginFailed
            shakeAnim.start()
        }
        onInformationMessage: {
            statusText.color = config.failureColor
            statusText.text = message
        }
    }

    Background {
        anchors.fill: parent
        source: config.background
        fillMode: Image.PreserveAspectCrop
        onStatusChanged: {
            if (status == Image.Error && source != config.defaultBackground) {
                source = config.defaultBackground
            }
        }
    }

    Rectangle {
        anchors.fill: parent
        color: root.bgBase
        opacity: 0.55
    }

    Clock {
        id: clock
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.margins: 28
        color: root.fgAccent
        timeFont.family: root.fontFamily
        timeFont.pointSize: 26
        dateFont.family: root.fontFamily
        dateFont.pointSize: 11
        opacity: 0

        NumberAnimation on opacity { to: 1; duration: 500; easing.type: Easing.OutQuad }
    }

    /* Мягкая псевдо-тень под карточкой: пара увеличенных полупрозрачных
       прямоугольников за карточкой (QtGraphicalEffects недоступен в Qt5-гритере) */
    Rectangle {
        anchors.centerIn: card
        width: card.width + 16
        height: card.height + 16
        radius: card.radius + 6
        color: "#000000"
        opacity: 0.10 * card.opacity
    }
    Rectangle {
        anchors.centerIn: card
        width: card.width + 8
        height: card.height + 8
        radius: card.radius + 3
        color: "#000000"
        opacity: 0.16 * card.opacity
    }

    Rectangle {
        id: card
        anchors.centerIn: parent
        width: 380
        radius: 10
        color: Qt.rgba(root.bgBase.r, root.bgBase.g, root.bgBase.b, 0.82)
        border.width: 2
        border.color: root.borderColor
        height: mainColumn.implicitHeight + 50

        scale: 0.94
        opacity: 0

        Behavior on border.color { ColorAnimation { duration: 200 } }

        SequentialAnimation {
            id: shakeAnim
            NumberAnimation { target: card; property: "anchors.horizontalCenterOffset"; to: -10; duration: 45 }
            NumberAnimation { target: card; property: "anchors.horizontalCenterOffset"; to: 10; duration: 45 }
            NumberAnimation { target: card; property: "anchors.horizontalCenterOffset"; to: -6; duration: 45 }
            NumberAnimation { target: card; property: "anchors.horizontalCenterOffset"; to: 6; duration: 45 }
            NumberAnimation { target: card; property: "anchors.horizontalCenterOffset"; to: 0; duration: 45 }
        }

        ParallelAnimation {
            id: introAnim
            running: true
            NumberAnimation { target: card; property: "opacity"; to: 1; duration: 420; easing.type: Easing.OutQuad }
            NumberAnimation { target: card; property: "scale"; to: 1; duration: 420; easing.type: Easing.OutBack; easing.overshoot: 4 }
        }

        Column {
            id: mainColumn
            anchors.centerIn: parent
            width: parent.width - 50
            spacing: 14

            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: sddm.hostName.length > 0 ? sddm.hostName : "vxwm"
                color: root.fgAccent
                font.family: root.fontFamily
                font.pixelSize: 16
            }

            TextBox {
                id: name
                width: parent.width; height: 36
                text: userModel.lastUser
                color: root.bgAlt
                borderColor: "transparent"
                focusColor: root.fgAccent
                hoverColor: root.fgAccent
                textColor: root.fgAccent
                radius: 6
                font.family: root.fontFamily
                font.pixelSize: 14

                KeyNavigation.backtab: shutdownButton; KeyNavigation.tab: password

                Keys.onPressed: {
                    if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                        sddm.login(name.text, password.text, sessionIndex)
                        event.accepted = true
                    }
                }

                Text {
                    anchors.left: parent.left
                    anchors.leftMargin: 10
                    anchors.verticalCenter: parent.verticalCenter
                    text: textConstants.userName
                    color: root.placeholderColor
                    font.family: root.fontFamily
                    font.pixelSize: 14
                    visible: name.text.length === 0 && !name.activeFocus
                }
            }

            PasswordBox {
                id: password
                width: parent.width; height: 36
                color: root.bgAlt
                borderColor: "transparent"
                focusColor: root.fgAccent
                hoverColor: root.fgAccent
                textColor: root.fgAccent
                radius: 6
                font.family: root.fontFamily
                font.pixelSize: 14

                KeyNavigation.backtab: name; KeyNavigation.tab: session

                Keys.onPressed: {
                    if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                        sddm.login(name.text, password.text, sessionIndex)
                        event.accepted = true
                    }
                }

                Text {
                    anchors.left: parent.left
                    anchors.leftMargin: 10
                    anchors.verticalCenter: parent.verticalCenter
                    text: textConstants.password
                    color: root.placeholderColor
                    font.family: root.fontFamily
                    font.pixelSize: 14
                    visible: password.text.length === 0 && !password.activeFocus
                }
            }

            Row {
                width: parent.width
                spacing: 8

                ComboBox {
                    id: session
                    width: parent.width * 0.6; height: 30
                    color: root.bgAlt
                    borderColor: "transparent"
                    focusColor: root.fgAccent
                    hoverColor: root.fgAccent
                    menuColor: root.bgAlt
                    textColor: root.fgBase
                    arrowColor: root.fgBase
                    arrowIcon: "chevron.svg"
                    font.family: root.fontFamily
                    font.pixelSize: 12

                    model: sessionModel
                    index: sessionModel.lastIndex

                    KeyNavigation.backtab: password; KeyNavigation.tab: layoutBox
                }

                LayoutBox {
                    id: layoutBox
                    width: parent.width * 0.4 - 8; height: 30
                    color: root.bgAlt
                    borderColor: "transparent"
                    focusColor: root.fgAccent
                    hoverColor: root.fgAccent
                    menuColor: root.bgAlt
                    textColor: root.fgBase
                    arrowColor: root.fgBase
                    arrowIcon: "chevron.svg"
                    font.family: root.fontFamily
                    font.pixelSize: 12

                    KeyNavigation.backtab: session; KeyNavigation.tab: loginButton
                }
            }

            Text {
                id: statusText
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: " "
                color: root.placeholderColor
                font.family: root.fontFamily
                font.pixelSize: 11
            }

            Row {
                width: parent.width
                spacing: 6

                /* Кол-во реально видимых кнопок может быть меньше 3 (canReboot/canPowerOff
                   бывают false), поэтому ширину считаем по видимым, а не жестко на 3 */
                property int visibleButtons: 1 + (sddm.canReboot ? 1 : 0) + (sddm.canPowerOff ? 1 : 0)
                property int btnWidth: (parent.width - spacing * (visibleButtons - 1)) / visibleButtons

                Button {
                    id: loginButton
                    width: parent.btnWidth; height: 32
                    text: textConstants.login
                    color: root.bgAlt
                    activeColor: root.fgAccent
                    pressedColor: root.fgAccent
                    textColor: root.fgBase
                    radius: 6
                    font.family: root.fontFamily
                    font.pixelSize: 12

                    onClicked: sddm.login(name.text, password.text, sessionIndex)
                    KeyNavigation.backtab: layoutBox; KeyNavigation.tab: rebootButton
                }

                Button {
                    id: rebootButton
                    width: parent.btnWidth; height: 32
                    text: textConstants.reboot
                    color: root.bgAlt
                    activeColor: root.fgAccent
                    pressedColor: root.fgAccent
                    textColor: root.fgBase
                    radius: 6
                    font.family: root.fontFamily
                    font.pixelSize: 12
                    visible: sddm.canReboot

                    onClicked: sddm.reboot()
                    KeyNavigation.backtab: loginButton; KeyNavigation.tab: shutdownButton
                }

                Button {
                    id: shutdownButton
                    width: parent.btnWidth; height: 32
                    text: textConstants.shutdown
                    color: root.bgAlt
                    activeColor: root.fgAccent
                    pressedColor: root.fgAccent
                    textColor: root.fgBase
                    radius: 6
                    font.family: root.fontFamily
                    font.pixelSize: 12
                    visible: sddm.canPowerOff

                    onClicked: sddm.powerOff()
                    KeyNavigation.backtab: rebootButton; KeyNavigation.tab: name
                }
            }
        }
    }

    Component.onCompleted: {
        if (name.text == "")
            name.focus = true
        else
            password.focus = true
    }
}
