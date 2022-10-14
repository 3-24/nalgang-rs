use std::{borrow::Cow, env, fmt};
use std::fmt::Write as FmtWrite;

use chrono::{DateTime, FixedOffset, NaiveDateTime};
use serenity::builder::CreateApplicationCommands;
use serenity::model::prelude::command::Command;
use serenity::{
    async_trait,
    client::{Context, EventHandler},
    model::{
        gateway::GatewayIntents,
        gateway::Ready,
        guild::{Guild, Member},
        id::GuildId,
        id::UserId,
        prelude::command::CommandOptionType,
        prelude::interaction::{
            application_command::{ApplicationCommandInteraction, CommandDataOptionValue},
            Interaction, InteractionResponseType,
        },
        timestamp::Timestamp,
    },
    Client,
};

struct Handler {
    database: sqlx::SqlitePool,
}

// Struct for database interaction
struct NalgangMember {
    pub uid: i64,
    pub gid: i64,
    pub score: Option<i64>,
    pub combo: Option<i64>,
    pub hit_time: Option<i64>,
}

// Wrapper for Serenity guild member
impl NalgangMember {
    pub fn new(member: &Member) -> Self {
        NalgangMember {
            uid: member.user.id.0 as i64,
            gid: member.guild_id.0 as i64,
            score: None,
            combo: None,
            hit_time: None,
        }
    }

    pub fn new_explict(user_id: UserId, guild_id: GuildId) -> Self {
        NalgangMember {
            uid: user_id.0 as i64,
            gid: guild_id.0 as i64,
            score: None,
            combo: None,
            hit_time: None,
        }
    }

    pub fn update_data(&mut self, score: i64, combo: i64, hit_time: i64) {
        self.score = Some(score);
        self.combo = Some(combo);
        self.hit_time = Some(hit_time);
    }
}

enum NalgangError {
    DuplicateAttendance,
    DuplicateMemberRegister,
    DuplicateGuildRegister,
    MemberNotExist,
    GuildNotExist,
    BufferError(std::fmt::Error),
    UnhandledDatabaseError {
        error: sqlx::Error,
        file: &'static str,
        line: u32,
    },
}

impl fmt::Display for NalgangError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            NalgangError::UnhandledDatabaseError { error, file, line } => {
                format!("UnhandledDatabaseError: {} at {}:{}", error, file, line)
            }
            NalgangError::MemberNotExist => "MemberNotExist".to_string(),
            NalgangError::DuplicateMemberRegister => "DuplicateMemberRegister".to_string(),
            NalgangError::DuplicateGuildRegister => "DuplicateGuildRegister".to_string(),
            NalgangError::DuplicateAttendance => "DuplicateAttendance".to_string(),
            _  => todo!()
        };
        write!(f, "{}", s)
    }
}

fn timestamp_round_down(utc_time: i64) -> i64 {
    let hour = 3600;
    let day = hour * 24;
    let utc_kst_offset = 9 * hour;
    let local_offset = 6 * hour;
    ((utc_time + utc_kst_offset - local_offset) / day) * day + local_offset - utc_kst_offset
}

fn earned_attendance_point(rank: i64, combo: i64) -> i64 {
    let mut earned_point = match rank {
        0 => 10,
        1 => 5,
        2 => 3,
        _ => 1,
    };

    if combo % 7 == 0 {
        earned_point += 20;
    }
    if combo % 30 == 0 {
        earned_point += 100;
    }
    if combo % 365 == 1500 {
        earned_point += 1500
    }
    earned_point
}

impl Handler {
    async fn get_member_info(&self, member: &mut NalgangMember) -> Result<bool, NalgangError> {
        let row = sqlx::query!(
            "SELECT score, combo, hit_time FROM Member WHERE user_id=? AND guild_id=? LIMIT 1",
            member.uid,
            member.gid
        )
        .fetch_one(&self.database)
        .await;

        match row {
            Ok(record) => {
                member.update_data(record.score, record.combo, record.hit_time);
                Ok(true)
            }
            Err(e) => match e {
                sqlx::Error::RowNotFound => Ok(false),
                _ => Err(NalgangError::UnhandledDatabaseError {
                    error: e,
                    file: file!(),
                    line: line!(),
                }),
            },
        }
    }

    async fn update_member_info(&self, member: &NalgangMember) -> Result<(), NalgangError> {
        let (score, combo, hit_time) = (
            member.score.unwrap(),
            member.combo.unwrap(),
            member.hit_time.unwrap(),
        );
        match sqlx::query!(
            "UPDATE Member SET score=?, combo=?, hit_time=? WHERE guild_id=? AND user_id=?",
            score,
            combo,
            hit_time,
            member.gid,
            member.uid
        )
        .execute(&self.database)
        .await
        {
            Ok(_) => Ok(()),
            Err(e) => Err(NalgangError::UnhandledDatabaseError {
                error: e,
                file: file!(),
                line: line!(),
            }),
        }
    }

    async fn daily_attendance_clear(&self, gid: i64) -> Result<(), NalgangError> {
        match sqlx::query!("DELETE FROM DailyAttendance WHERE guild_id=?", gid)
            .execute(&self.database)
            .await
        {
            Err(e) => Err(NalgangError::UnhandledDatabaseError {
                error: e,
                file: file!(),
                line: line!(),
            }),
            Ok(_) => Ok(()),
        }
    }

    async fn register_guild(&self, gid: i64) -> Result<(), NalgangError> {
        match sqlx::query_scalar!(
            "SELECT EXISTS (SELECT (1) FROM AttendanceTimeCount WHERE guild_id=? LIMIT 1)",
            gid
        )
        .fetch_one(&self.database)
        .await
        {
            Ok(1) => return Err(NalgangError::DuplicateGuildRegister),
            Ok(0) => (),
            Err(e) => {
                return Err(NalgangError::UnhandledDatabaseError {
                    error: e,
                    file: file!(),
                    line: line!(),
                })
            }
            _ => unreachable!(),
        };

        match sqlx::query!("INSERT INTO AttendanceTimeCount (guild_id) VALUES (?)", gid)
            .execute(&self.database)
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => Err(NalgangError::UnhandledDatabaseError {
                error: e,
                file: file!(),
                line: line!(),
            }),
        }
    }

    async fn command_point(&self, member: &mut NalgangMember) -> Result<(), NalgangError> {
        match self.get_member_info(member).await? {
            true => Ok(()),
            false => Err(NalgangError::MemberNotExist),
        }
    }

    async fn command_register(&self, member: &mut NalgangMember) -> Result<(), NalgangError> {
        if self.get_member_info(member).await? {
            return Err(NalgangError::DuplicateMemberRegister);
        }

        let (uid, gid) = (member.uid, member.gid);
        match sqlx::query!(
            "INSERT INTO Member (user_id, guild_id) VALUES (?, ?)",
            uid,
            gid
        )
        .execute(&self.database)
        .await
        {
            Ok(_) => Ok(()),
            Err(e) => Err(NalgangError::UnhandledDatabaseError {
                error: e,
                file: file!(),
                line: line!(),
            }),
        }
    }

    async fn command_nalgang(
        &self,
        member: &mut NalgangMember,
        time: Timestamp,
        message: String,
    ) -> Result<i64, NalgangError> {
        if !self.get_member_info(member).await? {
            return Err(NalgangError::MemberNotExist);
        }

        let (gid, uid, member_hit_time) = (member.gid, member.uid, member.hit_time.unwrap());
        let current_time = time.unix_timestamp();

        // Get last hit_count, hit_timestamp from AttendanceTimeCount by guild_id
        let guild_entry = sqlx::query!(
            "SELECT hit_count, hit_time FROM 
                AttendanceTimeCount WHERE guild_id=? LIMIT 1",
            gid
        )
        .fetch_one(&self.database)
        .await
        .map_err(|e| 
            match e {
                sqlx::Error::RowNotFound => NalgangError::GuildNotExist,
                _ => NalgangError::UnhandledDatabaseError {
                    error: e,
                    file: file!(),
                    line: line!(),
                }
            }
        )?;
        let guild_hit_count = guild_entry.hit_count;
        let guild_hit_time = guild_entry.hit_time;

        let day = 3600 * 24;
        let rank_boundary_time = timestamp_round_down(guild_hit_time) + day;
        let combo_boundary_time = timestamp_round_down(member_hit_time) + 2 * day;

        let rank = if current_time >= rank_boundary_time {
            self.daily_attendance_clear(gid).await?; // TODO: Schedule the delete query.
            0
        } else {
            // Raise error if user tries to do duplicate hit.
            // boundary_time - 86400 <= t < current_time < boundary_time, then duplicate hit!
            if (rank_boundary_time - 86400) <= member_hit_time {
                return Err(NalgangError::DuplicateAttendance);
            }
            guild_hit_count + 1
        };

        let _ = sqlx::query!(
            "UPDATE AttendanceTimeCount SET hit_count=?, hit_time=? WHERE guild_id=?",
            rank,
            current_time,
            gid
        )
        .execute(&self.database)
        .await
        .map_err(|e| NalgangError::UnhandledDatabaseError {
            error: e,
            file: file!(),
            line: line!(),
        })?;

        let combo = if current_time >= combo_boundary_time {
            1
        } else {
            member.combo.unwrap() + 1
        };
        let earned_point = earned_attendance_point(rank, combo);
        let new_score = member.score.unwrap() + earned_point;
        // Update Member
        member.update_data(new_score, combo, current_time);
        self.update_member_info(member).await?;

        // Update DailyAttendance
        let _ = sqlx::query!(
            "INSERT INTO DailyAttendance (guild_id, user_id, hit_message, hit_time) VALUES (?, ?, ?, ?)",
            gid, uid, message, current_time
        ).execute(&self.database).await.map_err(|e| NalgangError::UnhandledDatabaseError {
            error: e,
            file: file!(),
            line: line!(),
        })?;

        // Insert AttendanceHistory
        let _ = sqlx::query!(
            "INSERT INTO AttendanceHistory (guild_id, user_id, hit_message, hit_time, hit_score, hit_combo, hit_rank)
                VALUES (?, ?, ?, ?, ?, ?, ?)",
            gid, uid, message, current_time, new_score, combo, rank
        ).execute(&self.database).await.map_err(|e| NalgangError::UnhandledDatabaseError {
            error: e,
            file: file!(),
            line: line!(),
        })?;

        // TODO: Retrieve today's attendance board
        Ok(earned_point)
    }

    async fn simple_response(
        &self,
        ctx: &Context,
        command: &ApplicationCommandInteraction,
        message: Result<String, NalgangError>,
    ) {
        let content = match message {
            Ok(s) => s,
            Err(e) => {
                println!("{}", e);
                "오류가 발생했습니다.".to_string()
            }
        };

        if let Err(why) = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| message.content(content))
            })
            .await
        {
            println!("Cannot respond to slash command: {}", why);
        }
    }

    async fn today_attendance_collect(
        &self,
        context: &Context,
        guild_id: i64,
        current_time: Timestamp,
    ) -> Result<String, NalgangError> {
        let boundary_time = timestamp_round_down(current_time.unix_timestamp());

        let record = sqlx::query!(
            "SELECT user_id, hit_message FROM DailyAttendance WHERE guild_id=? AND hit_time >= ?",
            guild_id,
            boundary_time
        )
        .fetch_all(&self.database)
        .await;

        match record {
            Ok(rec) => {
                let mut content = String::new();
                let guild = GuildId(guild_id as u64);
                for (index, row) in rec.iter().enumerate() {
                    let user = UserId(row.user_id as u64);
                    let member = guild.member(context, user).await.unwrap();
                    let user_name = member.display_name();

                    let message = row.hit_message.clone().unwrap_or_default();
                    writeln!(&mut content, "{}. {}: {}", index + 1, user_name, message).map_err(NalgangError::BufferError)?;
                }
                Ok(content)
            }
            Err(e) => Err(NalgangError::UnhandledDatabaseError {
                error: e,
                file: file!(),
                line: line!(),
            }),
        }
    }

    async fn ranking_collect(&self, context: &Context, gid: i64) -> Result<String, NalgangError> {
        let record = sqlx::query!(
            "SELECT user_id, score FROM Member WHERE guild_id=? ORDER BY score DESC",
            gid,
        )
        .fetch_all(&self.database)
        .await;
        match record {
            Ok(rec) => {
                let mut content = String::new();
                let guild_id = GuildId(gid as u64);
                for (index, row) in rec.iter().enumerate() {
                    let user_id = UserId(row.user_id as u64);
                    let member = guild_id.member(context, user_id).await.unwrap();
                    let user_name = member.display_name();
                    writeln!(&mut content, "{}. {}점 {}", index + 1, row.score, user_name).map_err(NalgangError::BufferError)?;
                }

                Ok(content)
            }
            Err(e) => Err(NalgangError::UnhandledDatabaseError {
                error: e,
                file: file!(),
                line: line!(),
            }),
        }
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn guild_create(&self, _ctx: Context, guild: Guild, is_new: bool) {
        if is_new {
            match self.register_guild(guild.id.0 as i64).await {
                Ok(()) => (),
                Err(e) => {
                    println!("{}", e)
                }
            }
        }
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            let member = command.member.as_ref().expect("Expected guild member");
            let mut nalgang_member = NalgangMember::new(member);
            match command.data.name.as_str() {
                "서버등록" => {
                    let res = self.register_guild(nalgang_member.gid).await;
                    let content = match res {
                        Ok(()) => Ok("서버를 등록했습니다.".to_string()),
                        Err(e) => match e {
                            NalgangError::DuplicateGuildRegister => {
                                Ok("이미 등록된 서버입니다.".to_string())
                            }
                            _ => Err(e),
                        },
                    };
                    self.simple_response(&ctx, &command, content).await;
                }
                "등록" | "register" => {
                    let res = self.command_register(&mut nalgang_member).await;
                    let content = match res {
                        Ok(()) => Ok("계정을 등록했습니다.".to_string()),
                        Err(e) => match e {
                            NalgangError::DuplicateMemberRegister => Ok(format!(
                                "{}님은 이미 등록되었습니다.",
                                member.display_name()
                            )),
                            _ => Err(e),
                        },
                    };
                    self.simple_response(&ctx, &command, content).await;
                }

                "날갱" | "nalgang" => {
                    let interaction_time = command.id.created_at();

                    let message = match command.data.options.get(0) {
                        None => String::new(),
                        Some(v) => match v.resolved.as_ref().unwrap() {
                            CommandDataOptionValue::String(s) => s.clone(),
                            _ => unreachable!(),
                        },
                    };

                    let result = self
                        .command_nalgang(&mut nalgang_member, interaction_time, message)
                        .await;
                    match result {
                        Ok(earned_point) => {
                            let main_message = format!(
                                "{}님이 날갱해서 {}점을 얻었습니다!",
                                member.display_name(),
                                earned_point
                            );

                            let embed_result = self
                                .today_attendance_collect(
                                    &ctx,
                                    nalgang_member.gid,
                                    interaction_time,
                                )
                                .await;
                            match embed_result {
                                Ok(attendance_embed) => {
                                    let tz = FixedOffset::east(9 * 3600);
                                    let date = DateTime::<FixedOffset>::from_utc(
                                        NaiveDateTime::from_timestamp(
                                            interaction_time.unix_timestamp(),
                                            0,
                                        ),
                                        tz,
                                    )
                                    .date();

                                    if let Err(why) = command
                                    .create_interaction_response(&ctx.http, |response| {
                                        response
                                            .kind(InteractionResponseType::ChannelMessageWithSource)
                                            .interaction_response_data(|message|
                                                message
                                                .content(main_message)
                                                .embed(|create_embed| create_embed
                                                    .title("오늘의 날갱")
                                                    .field(date.format("%Y/%m/%d") , attendance_embed, false)
                                                ))
                                    }).await
                                {
                                    println!("Cannot respond to slash command: {}", why);
                                }
                                }
                                Err(e) => {
                                    println!("{}", e);
                                    self.simple_response(&ctx, &command, Err(e)).await;
                                }
                            };
                        }
                        Err(e) => {
                            let content = match e {
                                NalgangError::DuplicateAttendance => {
                                    Ok(format!("{}님은 이미 날갱했습니다.", member.display_name()))
                                }
                                NalgangError::MemberNotExist => {
                                    Ok("등록되지 않은 계정입니다.".to_string())
                                }
                                _ => Err(e),
                            };
                            self.simple_response(&ctx, &command, content).await;
                        }
                    }
                }
                "점수" => {
                    let (mut target_member, name) = match command.data.options.get(0) {
                        None => (nalgang_member, member.display_name()),
                        Some(value) => match value.resolved.as_ref().unwrap() {
                            CommandDataOptionValue::User(user, pm) => {
                                let display_name = match pm {
                                    Some(inner) => match inner.nick.as_ref() {
                                        Some(s) => Cow::Borrowed(s),
                                        None => Cow::Owned(user.name.clone()),
                                    },
                                    None => Cow::Owned(user.name.clone()),
                                };

                                (
                                    NalgangMember::new_explict(user.id, member.guild_id),
                                    display_name,
                                )
                            }
                            _ => unreachable!(),
                        },
                    };

                    let content = match self.command_point(&mut target_member).await {
                        Ok(()) => Ok(format!(
                            "{}님의 점수는 {}점입니다. {}연속 출석중입니다.",
                            name,
                            target_member.score.unwrap(),
                            target_member.combo.unwrap()
                        )),
                        Err(e) => match e {
                            NalgangError::MemberNotExist => {
                                Ok("등록되지 않은 계정입니다.".to_string())
                            }
                            _ => Err(e),
                        },
                    };
                    self.simple_response(&ctx, &command, content).await;
                }
                "랭킹" => {
                    let ranking_result = self.ranking_collect(&ctx, nalgang_member.gid).await;
                    match ranking_result {
                        Ok(ranking_result) => {
                            if let Err(why) = command
                                .create_interaction_response(&ctx.http, |response| {
                                    response
                                        .kind(InteractionResponseType::ChannelMessageWithSource)
                                        .interaction_response_data(|message| {
                                            message.embed(|create_embed| {
                                                create_embed
                                                    .title("랭킹")
                                                    .description(ranking_result)
                                            })
                                        })
                                })
                                .await
                            {
                                println!("Cannot respond to slash command: {}", why)
                            }
                        }
                        Err(e) => {
                            println!("{}", e);
                            self.simple_response(&ctx, &command, Err(e)).await;
                        }
                    }
                },
                _ => {
                    self.simple_response(&ctx, &command, Ok("개발 중인 기능입니다.".to_string())).await;
                }
            };
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        let _commands = Command::set_global_application_commands(&ctx.http, |commands: &mut CreateApplicationCommands| 
            commands
                .create_application_command(|command| {
                    command
                        .name("날갱")
                        .description("날갱합니다.")
                        .create_option(|option| {
                            option
                                .name("인사말")
                                .description("아무말이나 입력하세요.")
                                .kind(CommandOptionType::String)
                                .required(false)
                        })
                })
                .create_application_command(|command| {
                    command
                        .name("등록")
                        .description("날갱 시스템에 등록합니다.")
                })
                .create_application_command(|command| {
                    command
                        .name("점수")
                        .description("현재 날갱점수를 확인합니다.")
                        .create_option(|option| {
                            option
                                .name("이름")
                                .description("점수를 확인하고 싶은 계정을 입력해주세요.")
                                .kind(CommandOptionType::User)
                                .required(false)
                        })
                })
                .create_application_command(|command| {
                    command
                        .name("서버등록")
                        .description("서버를 날갱 시스템에 등록합니다.")
                })
                .create_application_command(|command| {
                    command.name("랭킹").description("순위를 확인합니다.")
                })
                .create_application_command(|command| {
                    command
                        .name("보내기")
                        .description("자신의 날갱점수를 다른 사람에게 보냅니다.")
                        .create_option(|option| {
                            option
                                .name("이름")
                                .description("점수를 보낼 계정을 입력해주세요.")
                                .kind(CommandOptionType::User)
                                .required(true)
                        })
                })
                .create_application_command(|command| {
                    command
                        .name("토큰발급")
                        .description("날갱 API를 이용할 수 있는 토큰을 발급합니다.")
                })
                .create_application_command(|command| {
                    command
                        .name("토큰삭제")
                        .description("소유 중인 API 토큰을 삭제합니다.")
                })
            ).await;
        // println!("{:?}", _commands.unwrap());
    }
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().expect("Failed to read .env file");

    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let database = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename("database.sqlite"))
        .await
        .expect("Couldn't connect to database");

    sqlx::migrate!("./migrations")
        .run(&database)
        .await
        .expect("Couldn't run database migrations");

    let handler = Handler { database };

    let application_id: u64 = env::var("APPLICATION_ID")
        .expect("Expected an application id in the environment")
        .parse()
        .expect("application id is not a valid id");

    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(token, intents)
        .event_handler(handler)
        .application_id(application_id)
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
