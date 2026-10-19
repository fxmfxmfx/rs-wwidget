use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    En,
    Ru,
}

pub struct Labels {
    pub now: &'static str,
    pub calendar: &'static str,
    pub system: &'static str,
    pub memory: &'static str,
    pub disk: &'static str,
    pub free: &'static str,
    pub months: [&'static str; 12],
    pub weekdays: [&'static str; 7],
    pub days: [&'static str; 7],
}

impl Language {
    pub const fn labels(self) -> &'static Labels {
        match self {
            Self::En => &EN,
            Self::Ru => &RU,
        }
    }
}

static EN: Labels = Labels {
    now: "NOW",
    calendar: "CALENDAR",
    system: "SYSTEM",
    memory: "RAM",
    disk: "DISK",
    free: "free",
    months: [
        "JANUARY",
        "FEBRUARY",
        "MARCH",
        "APRIL",
        "MAY",
        "JUNE",
        "JULY",
        "AUGUST",
        "SEPTEMBER",
        "OCTOBER",
        "NOVEMBER",
        "DECEMBER",
    ],
    weekdays: [
        "SUNDAY",
        "MONDAY",
        "TUESDAY",
        "WEDNESDAY",
        "THURSDAY",
        "FRIDAY",
        "SATURDAY",
    ],
    days: ["MO", "TU", "WE", "TH", "FR", "SA", "SU"],
};

static RU: Labels = Labels {
    now: "СЕЙЧАС",
    calendar: "КАЛЕНДАРЬ",
    system: "СИСТЕМА",
    memory: "ОЗУ",
    disk: "ДИСК",
    free: "своб.",
    months: [
        "ЯНВАРЬ",
        "ФЕВРАЛЬ",
        "МАРТ",
        "АПРЕЛЬ",
        "МАЙ",
        "ИЮНЬ",
        "ИЮЛЬ",
        "АВГУСТ",
        "СЕНТЯБРЬ",
        "ОКТЯБРЬ",
        "НОЯБРЬ",
        "ДЕКАБРЬ",
    ],
    weekdays: [
        "ВОСКРЕСЕНЬ",
        "ПОНЕДЕЛЬНИК",
        "ВТОРНИК",
        "СРЕДА",
        "ЧЕТВЕРГ",
        "ПЯТНИЦА",
        "СУББОТА",
    ],
    days: ["ПН", "ВТ", "СР", "ЧТ", "ПТ", "СБ", "ВС"],
};
