//! The note that opens each bot turn: the date and the time with their zone, who the bot is, and the
//! members of its channel. It goes in the message, not the system prompt, because it changes from one
//! turn to the next, and a bot has no tool to ask the time.

use chrono::{DateTime, Datelike, FixedOffset};

const WEEKDAYS: [&str; 7] = [
    "segunda-feira",
    "terça-feira",
    "quarta-feira",
    "quinta-feira",
    "sexta-feira",
    "sábado",
    "domingo",
];

/// The note for a turn at `now`. `members` lists a channel's members, and is empty in a bot's own chat.
pub fn note(
    now: DateTime<FixedOffset>,
    name: &str,
    team: Option<&str>,
    members: &[String],
) -> String {
    let weekday = WEEKDAYS[now.weekday().num_days_from_monday() as usize];
    let when = now.format("%d/%m/%Y, %H:%M");
    let zone = now.format("%:z");
    let who = match team {
        Some(team) => format!("{name} (equipe {team})"),
        None => name.to_owned(),
    };
    let mut lines = vec![
        format!("Agora: {weekday}, {when} (UTC{zone})."),
        format!("Você é {who}."),
    ];
    if !members.is_empty() {
        lines.push(format!("Membros do canal: {}.", members.join(", ")));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at_sao_paulo(hour: u32, minute: u32) -> DateTime<FixedOffset> {
        FixedOffset::west_opt(3 * 3600)
            .expect("a valid offset")
            .with_ymd_and_hms(2026, 10, 23, hour, minute, 0)
            .single()
            .expect("a real time")
    }

    #[test]
    fn the_note_gives_the_weekday_date_time_and_zone() {
        let text = note(at_sao_paulo(14, 5), "Ada", None, &[]);

        assert_eq!(
            text,
            "Agora: sexta-feira, 23/10/2026, 14:05 (UTC-03:00).\nVocê é Ada."
        );
    }

    #[test]
    fn a_team_and_the_channel_members_are_named() {
        let members = ["Ada (infra)".to_owned(), "Bento".to_owned()];

        let text = note(at_sao_paulo(9, 0), "Ada", Some("infra"), &members);

        assert_eq!(
            text,
            "Agora: sexta-feira, 23/10/2026, 09:00 (UTC-03:00).\nVocê é Ada (equipe infra).\nMembros do canal: Ada (infra), Bento."
        );
    }
}
