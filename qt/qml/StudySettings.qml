import QtQuick
import Reelhouse 1.0

// Study while watching: every way the player helps the user study, each one a switch of its own, so the user
// decides (settings general.study_*; core: studyplay.rs, wordmix.rs, mining.rs). Shown in Preferences (Study) and as
// a card of the Study mode; both write the same settings.
//
//     StudySettings { width: parent.width }
//
// Everything sits under the main switch "Study while watching" and needs the language being learned
// (general.study_lang, chosen under "Languages I know").
Column {
    id: ss
    width: parent ? parent.width : 600
    spacing: 18
    readonly property var g: Core.prefs && Core.prefs.general ? Core.prefs.general : ({})
    readonly property bool on: g.study_in_player === true
    readonly property bool noLang: (g.study_lang || "") === ""
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function set(path, value) { Core.cmd({cmd: "set_setting", path: path, value: value}) }
    // a setting that is on unless turned off (they all default to on)
    function yes(name) { return ss.g[name] !== false }
    function num(name, dflt) { return ss.g[name] !== undefined ? ss.g[name] : dflt }

    PrefGroup {
        title: ss.tr("Study while watching")
        description: ss.tr("Choose how the player tests what you are learning. Each kind can be turned on or off.")
        PrefSwitch {
            first: true
            title: ss.tr("Study while watching")
            subtitle: ss.noLang ? ss.tr("Choose the language you are learning in Preferences.") : ss.tr("Turns every kind below on or off")
            checked: ss.on
            onToggled: function(v) { ss.set("general.study_in_player", v) }
        }
        PrefSwitch {
            title: ss.tr("Flash cards in the subtitle")
            subtitle: ss.tr("When a word with kanji of the subtitle is in one of your Study decks, a small card with its reading and translation shows for 5 seconds")
            checked: ss.g.study_hints !== false
            onToggled: function(v) { ss.set("general.study_hints", v) }
        }
    }

    PrefGroup {
        visible: ss.on
        title: ss.tr("Questions about the subtitle")
        description: ss.tr("When the subtitle is in the language you are learning, about lines you should already know.")
        PrefSwitch {
            first: true
            title: ss.tr("Flash cards during videos")
            subtitle: ss.tr("After a line, the video pauses for a short question: its meaning, a word, its words in order, or a card of your decks")
            checked: ss.yes("study_quiz_cards")
            onToggled: function(v) { ss.set("general.study_quiz_cards", v) }
        }
        PrefSwitch {
            title: ss.tr("Parts without subtitles")
            subtitle: ss.tr("Now and then a part plays without subtitles; then you type what you heard")
            checked: ss.yes("study_quiz_listen")
            onToggled: function(v) { ss.set("general.study_quiz_listen", v) }
        }
        PrefSlider {
            visible: ss.yes("study_quiz_cards") || ss.yes("study_quiz_listen")
            title: ss.tr("How often")
            subtitle: ss.tr("100%: a question about every 2 minutes")
            from: 1; to: 100; step: 1
            value: ss.num("study_quiz_rate", 50)
            onMoved: function(v) { ss.set("general.study_quiz_rate", Math.round(v)) }
        }
    }

    PrefGroup {
        visible: ss.on
        title: ss.tr("Learned words in the subtitles")
        description: ss.tr("When the subtitle is in a language you know, a word or two of a line become the words you learned, so they come back naturally.")
        PrefSwitch {
            first: true
            title: ss.tr("Mix learned words into subtitles")
            subtitle: ss.tr("Only words you remember well; the mixed words are shown in another colour")
            checked: ss.yes("study_wordmix")
            onToggled: function(v) { ss.set("general.study_wordmix", v) }
        }
        PrefSlider {
            visible: ss.yes("study_wordmix")
            title: ss.tr("Lines with learned words")
            from: 1; to: 100; step: 1
            value: ss.num("study_wordmix_rate", 50)
            onMoved: function(v) { ss.set("general.study_wordmix_rate", Math.round(v)) }
        }
        PrefSwitch {
            visible: ss.yes("study_wordmix")
            title: ss.tr("Show the reading")
            subtitle: ss.tr("Japanese words get their reading in kana, small, beside them")
            checked: ss.yes("study_wordmix_reading")
            onToggled: function(v) { ss.set("general.study_wordmix_reading", v) }
        }
        PrefSwitch {
            visible: ss.yes("study_wordmix")
            title: ss.tr("Only words said in the audio")
            subtitle: ss.tr("A word is swapped only when the subtitle in the language of the audio (built in, a file or from your addons) has it at that moment, copied as it is said there. Without that subtitle, no word is swapped.")
            checked: ss.yes("study_wordmix_confirm")
            onToggled: function(v) { ss.set("general.study_wordmix_confirm", v) }
        }
    }

    PrefGroup {
        visible: ss.on
        title: ss.tr("Cards from the episode")
        description: ss.tr("When the audio is in the language you are learning and the subtitle in one you know, words said alone between silences become cards with their clip, in the deck Seu Reels.")
        PrefSwitch {
            first: true
            title: ss.tr("Make cards from single words")
            subtitle: ss.tr("Runs in the background while you watch; a message says how many were made")
            checked: ss.yes("study_mining")
            onToggled: function(v) { ss.set("general.study_mining", v) }
        }
        PrefSpin {
            visible: ss.yes("study_mining")
            title: ss.tr("Cards per episode")
            from: 1; to: 50; step: 1
            value: ss.num("study_mining_max", 10)
            onMoved: function(v) { ss.set("general.study_mining_max", Math.round(v)) }
        }
    }

    PrefGroup {
        visible: ss.on
        title: ss.tr("The macaw in the player")
        PrefSwitch {
            first: true
            title: ss.tr("The macaw helps while you watch")
            subtitle: ss.tr("It brings the question cards, keeps quiet in a corner during the parts without subtitles and peeks at the learned words")
            checked: ss.yes("study_mascot_cues")
            onToggled: function(v) { ss.set("general.study_mascot_cues", v) }
        }
    }
}
