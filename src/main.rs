use std::env;

use serenity::{
    async_trait,
    client::{Context, EventHandler},
    model::{
        gateway::GatewayIntents,
        gateway::Ready,
        guild::Guild,
        id::GuildId,
        id::UserId,
        prelude::command::CommandOptionType,
        prelude::interaction::{
            application_command::CommandDataOptionValue, Interaction, InteractionResponseType,
        },
        user::User,
        timestamp::Timestamp
    },
    Client,
};

struct Handler {
    database: sqlx::SqlitePool,
}

struct CommandMember {
    pub nick: String,
    pub user: User,
}

struct NalgangMember {
    pub user_id: UserId,
    pub guild_id: GuildId,
    pub score: Option<i64>,
    pub combo: Option<i64>
}

impl NalgangMember {
    pub fn new(user_id: UserId, guild_id: GuildId) -> Self {
        NalgangMember { user_id, guild_id, score: None, combo: None }
    }

    pub fn get_uid(&self) -> i64 {
        self.user_id.0 as i64
    }

    pub fn get_gid(&self) -> i64 {
        self.guild_id.0 as i64
    }

    pub fn update_score_and_combo(&mut self, score: i64, combo: i64) {
        self.score = Some(score);
        self.combo = Some(combo);
    }
}

enum NalgangError {
    DuplicateAttendance,
    DuplicateRegister,
    MemberNotExist,
    UnhandledDatabaseError(sqlx::Error),
}

impl Handler {
    async fn get_member_info(
        &self,
        member: &mut NalgangMember
    ) -> Result<bool, NalgangError> {
        let uid = member.get_uid();
        let gid = member.get_gid();
        let row = sqlx::query!(
            "SELECT score, combo FROM Member WHERE user_id=? AND guild_id=? LIMIT 1", uid, gid
        ).fetch_one(&self.database).await;
        
        match row {
            Ok(record) => {
                member.update_score_and_combo(record.score, record.combo); Ok(true)
            },
            Err(e) => match e {
                sqlx::Error::RowNotFound => Ok(false),
                _ => Err(NalgangError::UnhandledDatabaseError(e))
            }
        }
    }

    async fn command_register(
        &self,
        member: &mut NalgangMember
    ) -> Result<(), NalgangError> {

        if self.get_member_info(member).await? {
            return Err(NalgangError::DuplicateRegister)
        }

        let (uid, gid) = (member.get_uid(), member.get_gid());
        match sqlx::query!(
            "INSERT INTO Member (user_id, guild_id) VALUES (?, ?)", uid, gid
        )
        .execute(&self.database).await {
            Ok(_) => Ok(()),
            Err(e) => Err(NalgangError::UnhandledDatabaseError(e))
        }
    }

    async fn command_nalgang(
        &self,
        member: &mut NalgangMember,
        time: Timestamp,
        message: String,
    ) -> Result<String, NalgangError> {

        if !self.get_member_info(member).await? {
            return Err(NalgangError::MemberNotExist)
        }

        let (gid, uid) = (member.get_gid(), member.get_uid());
        let current_time = time.unix_timestamp();

        let user_hit_entry = sqlx::query!(
            "SELECT hit_time FROM DailyAttendance WHERE guild_id=? AND user_id=? LIMIT 1",
            gid, uid)
            .fetch_one(&self.database)
            .await;

        // Get last hit_count, hit_timestamp from AttendanceTimeCount by guild_id
        let guild_entry = sqlx::query!(
            "SELECT hit_count, hit_time FROM 
                AttendanceTimeCount WHERE guild_id=? LIMIT 1",
            gid
        )
        .fetch_one(&self.database)
        .await;

        let (hit_rank, combo) = match guild_entry {
            Ok(x) => {
                let last_hit_count = x.hit_count;
                let last_hit_time = x.hit_time;
                // Closest to KST 6:00 AM
                let boundary_time = ((last_hit_time - 43200 + 86400 - 1) / 86400) * 86400;

                let rank = if current_time >= boundary_time {
                    1
                } else {
                    // Raise error if user tries to do duplicate hit.
                    match user_hit_entry {
                        Ok(y) => {
                            // boundary_time - 86400 <= t < current_time < boundary_time, then duplicate hit!
                            let t = y.hit_time;
                            if (boundary_time - 86400) <= t {
                                return Err(NalgangError::DuplicateAttendance);
                            }
                        }
                        Err(e) => {
                            return Err(NalgangError::UnhandledDatabaseError(e));
                        }
                    }
                    last_hit_count + 1
                };

                sqlx::query!(
                    "UPDATE AttendanceTimeCount SET hit_count=?, hit_time=? WHERE guild_id=?",
                    rank,
                    current_time,
                    gid
                )
                .execute(&self.database)
                .await
                .or_else(|e| return Err(NalgangError::UnhandledDatabaseError(e)));

                (rank, 1)
            }
            Err(e) => {
                match e {
                    sqlx::Error::RowNotFound => {
                        sqlx::query!(
                            "INSERT INTO AttendanceTimeCount (guild_id, hit_count, hit_time) VALUES (?, ?, ?)",
                            gid, 1, current_time
                        )
                        .execute(&self.database).await.unwrap();
                    }
                    _ => {
                        return Err(NalgangError::UnhandledDatabaseError(e));
                    }
                }
                (1, 1)
            }
        };

        // If (KST 6:00) <= time /\ time < (KST 6:00) + 1 day, combo += 1. Otherwise, combo = 0
        /*
        let combo = match user_hit_entry {
            Ok(x) => {
                let user_hit_time = x.hit_time;
                let user_hit_boundary_time = ((user_hit_time - 43200 + 86400 - 1) / 86400) * 86400;
                todo!()
            }
            Err(e) => todo!()
        };*/

        // get score, combo from Members by (user_id, guild_id)
        /*
        let entry = sqlx::query!(
            "SELECT score, combo FROM Members WHERE guild_id=? AND user_id=? LIMIT 1"
        );
        */
        // Insert hit_message, hit_timestamp into Attendances

        // Based on hit_count and hit_timestamp, calculate the point to be added.
        // Craft a message with combo, added point.
        // Insert combo, added point into Members
        todo!()
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn guild_create(&self, _ctx: Context, guild: Guild, is_new: bool) {
        if is_new {
            let gid = guild.id.0 as i64;
            println!("Get new invite from guild {}", gid);
            sqlx::query!("INSERT INTO AttendanceTimeCount (guild_id) VALUES (?)", gid)
                .execute(&self.database)
                .await
                .unwrap();
        }
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            let guild_id = command.member.as_ref().unwrap().guild_id;
            let member = command.member.as_ref().expect("Expected guild member");
            let mut nalgang_member = NalgangMember::new(member.user.id, guild_id);
            let command_result = match command.data.name.as_str() {
                "등록" | "register" => {
                    let res = self.command_register(&mut nalgang_member).await;
                    match res {
                        Ok(()) => Ok("등록되었습니다".to_string()),
                        Err(e) => match e {
                            NalgangError::DuplicateRegister => Ok("이미 등록되었습니다".to_string()),
                            _ => todo!()
                        }
                    }
                }

                "날갱" | "nalgang" => {
                    let interaction_time = command.id.created_at();
                    let result = self
                        .command_nalgang(
                            &mut nalgang_member,
                            interaction_time,
                            "Test".to_string(),
                        )
                        .await;
                    todo!()
                }
                "점수" => {
                    let member: Result<CommandMember, String> = match command.data.options.get(0) {
                        None => {
                            let member = command.member.as_ref().expect("Expected guild member");
                            Ok(CommandMember {
                                nick: member.nick.clone().unwrap(),
                                user: member.user.clone(),
                            })
                        }
                        Some(value) => {
                            let x = value.resolved.as_ref().expect("Expected user object");
                            match x {
                                CommandDataOptionValue::User(user, member) => match member {
                                    Some(pm) => Ok(CommandMember {
                                        nick: pm.nick.clone().unwrap_or_else(|| user.name.clone()),
                                        user: user.clone(),
                                    }),
                                    _ => Err("Please provide a guild member".to_string()),
                                },
                                _ => Err("Please provide a valid user".to_string()),
                            }
                        }
                    };
                    match member {
                        Ok(m) => Ok(format!("{}'s id is {}", m.nick, m.user.id)),
                        Err(s) => Err(s),
                    }
                } // TODO: 데이터베이스와 상호작용하기
                /*
                "등록" => "등록하기".to_string(), // TODO
                "보내기" => "보내기".to_string(), // TODO
                "순위표" | "점수표" | "순위" => "순위 출력하기".to_string(),
                "점수추가" => "점수를 추가하기".to_string(), //TODO
                */
                _ => Ok("Not implemented :(".to_string()),
            };

            let content = command_result.unwrap_or("오류가 발생했습니다.".to_string());
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
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        let guild_id = GuildId(
            env::var("GUILD_ID")
                .expect("Expected GUILD_ID in environment")
                .parse()
                .expect("GUILD_ID must be an integer"),
        );

        let _commands = GuildId::set_application_commands(&guild_id, &ctx.http, |commands| {
            commands
                .create_application_command(|command| {
                    command.name("날갱").description("날갱합니다.")
                })
                .create_application_command(|command| {
                    command.name("등록").description("날갱 시스템에 등록합니다.")
                })
                .create_application_command(|command| {
                    command
                        .name("점수")
                        .description("현재 날갱점수를 확인합니다.")
                        .create_option(|option| {
                            option
                                .name("이름")
                                .description("점수를 확인할 사용자")
                                .kind(CommandOptionType::User)
                                .required(false)
                        })
                })
        })
        .await;
    }
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().expect("Failed to read .env file");
    // Configure the client with your Discord bot token in the environment.
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

    // The Application Id is usually the Bot User Id.
    let application_id: u64 = env::var("APPLICATION_ID")
        .expect("Expected an application id in the environment")
        .parse()
        .expect("application id is not a valid id");

    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    // Build our client.
    let mut client = Client::builder(token, intents)
        .event_handler(handler)
        .application_id(application_id)
        .await
        .expect("Error creating client");

    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform
    // exponential backoff until it reconnects.
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
