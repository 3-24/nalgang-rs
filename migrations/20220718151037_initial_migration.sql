-- Add migration script here

CREATE TABLE IF NOT EXISTS Member
(
    guild_id integer NOT NULL,
    user_id integer NOT NULL,
    score integer NOT NULL DEFAULT 0,
    combo integer NOT NULL DEFAULT 0,
    hit_time integer NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS DailyAttendance
(
    guild_id integer NOT NULL,
    user_id integer NOT NULL,
    hit_message nvarchar,
    hit_time integer NOT NULL
);

CREATE TABLE IF NOT EXISTS AttendanceTimeCount
(
    guild_id integer NOT NULL,
    hit_count integer NOT NULL DEFAULT 0,
    hit_time integer NOT NULL DEFAULT 0
);

/* Stack all attendances */
CREATE TABLE IF NOT EXISTS AttendanceHistory
(
    guild_id integer NOT NULL,
    user_id integer NOT NULL,
    hit_message nvarchar,
    hit_time integer NOT NULL,
    hit_score integer NOT NULL,
    hit_combo integer NOT NULL,
    hit_rank integer NOT NULL
);

CREATE TABLE IF NOT EXISTS Token
(
    guild_id integer NOT NULL,
    user_id integer NOT NULL,
    token char(32),
    primary key(guild_id, user_id)
)