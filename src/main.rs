use std::env;

use serenity::{
    async_trait,
    model::{
        gateway::Ready,
        id::GuildId,
        interactions::{
            application_command::{
                ApplicationCommand,
                ApplicationCommandInteractionDataOptionValue,
                ApplicationCommandOptionType,
            },
            Interaction,
            InteractionResponseType,
        },
    },
    prelude::*,
};

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            let content = match command.data.name.as_str() {
                "날갱" | "nalgang" => "날갱하기".to_string(), // TODO
                "점수" => {
                    let options = command.data.options.get(0).expect("Expected user option").resolved.
                    println!()
                    "점수를 알려주기".to_string()
                }, // TODO
                /*
                "등록" => "등록하기".to_string(), // TODO
                "보내기" => "보내기".to_string(), // TODO
                "순위표" | "점수표" | "순위" => "순위 출력하기".to_string(),
                "점수추가" => "점수를 추가하기".to_string(), //TODO
                */
                _ => "Not implemented :(".to_string()
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
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        let guild_id = GuildId(
            env::var("GUILD_ID")
                .expect("Expected GUILD_ID in environment")
                .parse()
                .expect("GUILD_ID must be an integer"),
        );

        let commands = GuildId::set_application_commands(&guild_id, &ctx.http, |commands| {
            commands
                .create_application_command(|command| {
                    command.name("날갱").description("날갱합니다.")
                })
                .create_application_command(|command| {
                    command.name("점수").description("현재 날갱점수를 확인합니다.").create_option(|option| {
                        option
                        .name("이름")
                        .description("점수를 확인할 사용자")
                        .kind(ApplicationCommandOptionType::User)
                        .required(false)
                    })
                })
        })
        .await;

        println!("I now have the following guild slash commands: {:#?}", commands);

        let guild_command =
            ApplicationCommand::create_global_application_command(&ctx.http, |command| {
                command.name("wonderful_command").description("An amazing command")
            })
            .await;

        println!("I created the following global slash command: {:#?}", guild_command);
    }
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().expect("Failed to read .env file");
    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    // The Application Id is usually the Bot User Id.
    let application_id: u64 = env::var("APPLICATION_ID")
        .expect("Expected an application id in the environment")
        .parse()
        .expect("application id is not a valid id");

    // Build our client.
    let mut client = Client::builder(token)
        .event_handler(Handler)
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