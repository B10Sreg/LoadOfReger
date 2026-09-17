import QtQuick 2.15
import SddmComponents 2.0

/* Экран входа. Цвета приходят из theme.conf, который собирается из палитры
   темы (vxwm/rice/templates/sddm-theme.conf.tmpl): греетер работает под root
   до всякой сессии, поэтому ни рис, ни Xresources ему недоступны -- всё, что
   он знает о теме, лежит рядом с ним файлом.

   Компоненты SddmComponents (TextBox, PasswordBox, ComboBox, LayoutBox,
   Button) заменить нечем: только они умеют разговаривать с греетером. Всё
   остальное -- обычный QtQuick. */
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
    property color fgBright: config.fgBright
    property color fgAccent: config.fgAccent
    property color borderColor: config.borderColor
    property color placeholderColor: config.placeholderColor
    property string fontFamily: config.fontFamily

    /* Иконки Nerd Font. Записаны escape-последовательностями, а не самими
       глифами: файл остаётся ASCII и переживает любой редактор, а глиф из
       приватной области легко теряется при копировании. Диапазон U+F0xx --
       это Font Awesome внутри Nerd Font, он есть в BMP; значки Material
       Design, которыми пользуется бар, лежат выше U+FFFF, и в QML их
       пришлось бы писать суррогатной парой. */
    readonly property string iconUser: "\uf007"
    readonly property string iconLock: "\uf023"
    readonly property string iconPower: "\uf011"
    readonly property string iconReboot: "\uf021"
    readonly property string iconHost: "\uf108"
    readonly property string iconKeyboard: "\uf11c"

    TextConstants { id: textConstants }

    Connections {
        target: sddm
        onLoginSucceeded: {
            statusText.color = config.successColor
            statusText.text = textConstants.loginSucceeded
        }
        onLoginFailed: {
            password.text = ""
            statusText.color = config.failureColor
            statusText.text = textConstants.loginFailed
            shakeAnim.start()
            password.focus = true
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

    /* Затемнение обоев: карточка должна читаться на любой картинке, а не
       только на тёмной. Снизу гуще -- там подпись и панель питания. */
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: Qt.rgba(root.bgBase.r, root.bgBase.g, root.bgBase.b, 0.50) }
            GradientStop { position: 0.55; color: Qt.rgba(root.bgBase.r, root.bgBase.g, root.bgBase.b, 0.62) }
            GradientStop { position: 1.0; color: Qt.rgba(root.bgBase.r, root.bgBase.g, root.bgBase.b, 0.86) }
        }
    }

    /* ------------------------------------------------------------ часы */

    /* Свои часы вместо компонента Clock: тому нельзя задать локаль, и дата
       выходила английской посреди русского интерфейса. Формат тот же, что у
       часов в баре. */
    Column {
        id: clock
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: parent.top
        anchors.topMargin: Math.round(root.height * 0.13)
        spacing: 2
        opacity: 0

        property date now: new Date()

        Timer {
            interval: 1000
            running: true
            repeat: true
            onTriggered: clock.now = new Date()
        }

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: clock.now.toLocaleTimeString(Qt.locale("ru_RU"), "HH:mm")
            color: root.fgBright
            font.family: root.fontFamily
            font.pixelSize: 72
            font.letterSpacing: 4
        }

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: clock.now.toLocaleDateString(Qt.locale("ru_RU"), "dddd, d MMMM")
            color: root.fgBase
            font.family: root.fontFamily
            font.pixelSize: 14
            font.letterSpacing: 2
            opacity: 0.9
        }

        NumberAnimation on opacity { to: 1; duration: 700; easing.type: Easing.OutQuad }
    }

    /* --------------------------------------------------------- карточка */

    /* Мягкая псевдо-тень: пара увеличенных полупрозрачных прямоугольников за
       карточкой. QtGraphicalEffects в греетере недоступен, а без тени
       карточка на светлых обоях выглядит наклейкой. */
    Rectangle {
        anchors.centerIn: card
        width: card.width + 24
        height: card.height + 24
        radius: card.radius + 10
        color: "#000000"
        opacity: 0.10 * card.opacity
    }
    Rectangle {
        anchors.centerIn: card
        width: card.width + 12
        height: card.height + 12
        radius: card.radius + 5
        color: "#000000"
        opacity: 0.20 * card.opacity
    }

    Rectangle {
        id: card
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: clock.bottom
        anchors.topMargin: 48
        width: 420
        radius: 14
        color: Qt.rgba(root.bgBase.r, root.bgBase.g, root.bgBase.b, 0.88)
        /* Рамка в цвет плашек, акцентная -- только когда ввод в фокусе:
           постоянная светлая обводка спорит с аватаром и кнопкой входа. */
        border.width: 1
        border.color: (name.activeFocus || password.activeFocus)
                      ? Qt.rgba(root.borderColor.r, root.borderColor.g, root.borderColor.b, 0.45)
                      : root.bgAlt
        height: mainColumn.implicitHeight + 60

        scale: 0.94
        opacity: 0

        Behavior on border.color { ColorAnimation { duration: 180 } }

        /* Акцентная полоса по верхней кромке: единственная яркая линия на
           экране, она же связывает карточку с рамкой активного окна в самой
           сессии. Ширина ведёт себя как индикатор -- растёт, когда форма
           готова к вводу пароля. */
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            y: 0
            height: 2
            radius: 1
            width: password.activeFocus ? parent.width * 0.6
                 : name.activeFocus ? parent.width * 0.3
                 : parent.width * 0.12
            color: root.fgAccent
            opacity: 0.9
            Behavior on width { NumberAnimation { duration: 220; easing.type: Easing.OutCubic } }
        }

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
            NumberAnimation { target: card; property: "scale"; to: 1; duration: 420; easing.type: Easing.OutBack; easing.overshoot: 3 }
        }

        Column {
            id: mainColumn
            anchors.centerIn: parent
            width: parent.width - 64
            spacing: 10

            /* Аватар: картинку пользователя греетер отдаёт не всегда, поэтому
               под ней всегда лежит кружок с буквой -- пустого места не будет. */
            Item {
                width: parent.width
                height: 84

                /* Ореол вокруг аватара: тот же акцент, что у рамки, но еле
                   заметный -- он собирает взгляд на центре карточки. */
                Rectangle {
                    anchors.centerIn: avatar
                    width: avatar.width + 16
                    height: avatar.height + 16
                    radius: width / 2
                    color: "transparent"
                    border.width: 1
                    border.color: root.fgAccent
                    opacity: 0.20
                }

                Rectangle {
                    id: avatar
                    anchors.centerIn: parent
                    width: 76; height: 76
                    radius: width / 2
                    color: root.bgAlt
                    border.width: 1
                    border.color: root.borderColor

                    Text {
                        anchors.centerIn: parent
                        visible: face.status !== Image.Ready
                        text: name.text.length > 0 ? name.text.charAt(0).toUpperCase() : root.iconUser
                        color: root.fgAccent
                        font.family: root.fontFamily
                        font.pixelSize: 30
                    }

                    Image {
                        id: face
                        anchors.fill: parent
                        anchors.margins: 2
                        fillMode: Image.PreserveAspectCrop
                        smooth: true
                        visible: status === Image.Ready
                        source: name.text.length > 0
                                ? "file:///var/lib/AccountsService/icons/" + name.text
                                : ""
                    }
                }
            }

            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: name.text.length > 0 ? name.text : textConstants.userName
                color: root.fgBright
                font.family: root.fontFamily
                font.pixelSize: 17
            }

            Item { width: 1; height: 6 }

            TextBox {
                id: name
                width: parent.width; height: 40
                text: userModel.lastUser
                color: root.bgAlt
                borderColor: "transparent"
                focusColor: root.fgAccent
                hoverColor: root.fgAccent
                textColor: root.fgBright
                radius: 9
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
                    anchors.leftMargin: 12
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.iconUser + "  " + textConstants.userName
                    color: root.placeholderColor
                    font.family: root.fontFamily
                    font.pixelSize: 13
                    visible: name.text.length === 0 && !name.activeFocus
                }
            }

            PasswordBox {
                id: password
                width: parent.width; height: 40
                color: root.bgAlt
                borderColor: "transparent"
                focusColor: root.fgAccent
                hoverColor: root.fgAccent
                textColor: root.fgBright
                radius: 9
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
                    anchors.leftMargin: 12
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.iconLock + "  " + textConstants.password
                    color: root.placeholderColor
                    font.family: root.fontFamily
                    font.pixelSize: 13
                    visible: password.text.length === 0 && !password.activeFocus
                }
            }

            /* Сессия и раскладка. LayoutBox рисует флаг слева сам, поэтому
               места ему нужно меньше, чем названию сессии, но не вдвое. */
            Row {
                width: parent.width
                spacing: 8

                ComboBox {
                    id: session
                    width: Math.round((parent.width - 8) * 0.62); height: 34
                    color: root.bgAlt
                    borderColor: "transparent"
                    focusColor: root.fgAccent
                    hoverColor: root.fgAccent
                    menuColor: root.bgAlt
                    textColor: root.fgBase
                    arrowColor: root.bgAlt
                    arrowIcon: "chevron.svg"
                    font.family: root.fontFamily
                    font.pixelSize: 12

                    model: sessionModel
                    index: sessionModel.lastIndex

                    KeyNavigation.backtab: password; KeyNavigation.tab: layoutBox
                }

                /* Раскладка: свой делегат вместо стандартного. Тот рисует
                   флаг страны -- цветной прямоугольник посреди монохромного
                   экрана, да ещё и наезжающий на подпись. Значок клавиатуры и
                   код раскладки говорят то же самое и не спорят с темой. */
                LayoutBox {
                    id: layoutBox
                    width: (parent.width - 8) - session.width; height: 34

                    rowDelegate: Rectangle {
                        color: "transparent"
                        Text {
                            anchors.left: parent.left
                            anchors.leftMargin: 12
                            anchors.verticalCenter: parent.verticalCenter
                            text: root.iconKeyboard + "  " + (modelItem
                                  ? modelItem.modelData.shortName.toUpperCase() : "??")
                            color: root.fgBase
                            font.family: root.fontFamily
                            font.pixelSize: 12
                        }
                    }
                    color: root.bgAlt
                    borderColor: "transparent"
                    focusColor: root.fgAccent
                    hoverColor: root.fgAccent
                    menuColor: root.bgAlt
                    textColor: root.fgBase
                    arrowColor: root.bgAlt
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
                elide: Text.ElideRight
            }

            /* Главное действие -- единственная залитая акцентом кнопка на
               экране. Текст на ней тёмный: акцент светлый, и одноцветный с
               ним текст пропадал бы при наведении. */
            Button {
                id: loginButton
                width: parent.width; height: 40
                text: textConstants.login
                color: root.fgAccent
                activeColor: Qt.lighter(root.fgAccent, 1.12)
                pressedColor: Qt.darker(root.fgAccent, 1.12)
                textColor: root.bgBase
                radius: 9
                font.family: root.fontFamily
                font.pixelSize: 14

                onClicked: sddm.login(name.text, password.text, sessionIndex)
                KeyNavigation.backtab: layoutBox; KeyNavigation.tab: rebootButton
            }
        }
    }

    /* ---------------------------------------------------------- питание */

    /* Перезагрузка и выключение вынесены из карточки в угол экрана: в форме
       входа они стояли рядом с «Войти» и весили столько же, хотя нажимают их
       на порядок реже. */
    Row {
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 28
        spacing: 10
        opacity: 0.85

        Button {
            id: rebootButton
            width: 44; height: 44
            text: root.iconReboot
            color: Qt.rgba(root.bgAlt.r, root.bgAlt.g, root.bgAlt.b, 0.8)
            activeColor: root.fgAccent
            pressedColor: Qt.darker(root.fgAccent, 1.12)
            textColor: (rebootButton.isFocused || rebootButton.isPressed) ? root.bgBase : root.fgBase
            radius: 22
            font.family: root.fontFamily
            font.pixelSize: 16
            visible: sddm.canReboot

            onClicked: sddm.reboot()
            KeyNavigation.backtab: loginButton; KeyNavigation.tab: shutdownButton
        }

        Button {
            id: shutdownButton
            width: 44; height: 44
            text: root.iconPower
            color: Qt.rgba(root.bgAlt.r, root.bgAlt.g, root.bgAlt.b, 0.8)
            activeColor: config.failureColor
            pressedColor: Qt.darker(config.failureColor, 1.12)
            textColor: (shutdownButton.isFocused || shutdownButton.isPressed) ? root.bgBase : root.fgBase
            radius: 22
            font.family: root.fontFamily
            font.pixelSize: 16
            visible: sddm.canPowerOff

            onClicked: sddm.powerOff()
            KeyNavigation.backtab: rebootButton; KeyNavigation.tab: name
        }
    }

    /* Имя машины -- в углу, а не в карточке: на одном компьютере оно всё
       время одно и то же и в форме входа только занимает строку. */
    Text {
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        anchors.margins: 28
        text: root.iconHost + "  " + (sddm.hostName.length > 0 ? sddm.hostName : "vxwm")
        color: root.fgBase
        font.family: root.fontFamily
        font.pixelSize: 12
        font.letterSpacing: 1
        opacity: 0.55
    }

    Component.onCompleted: {
        if (name.text == "")
            name.focus = true
        else
            password.focus = true
    }
}
