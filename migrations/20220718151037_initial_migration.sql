-- Add migration script here
CREATE TABLE IF NOT EXISTS Members
(
    id integer NOT NULL,
    guild integer NOT NULL,
    score integer,
    combo integer
);

CREATE TABLE IF NOT EXISTS Attendances
(
    id integer NOT NULL,
    guild integer NOT NULL,
    hit_message nvarchar,
    hit_timestamp float NOT NULL
);

CREATE TABLE IF NOT EXISTS AttendanceTimeCount
(
    guild integer NOT NULL,
    hit_count integer NOT NULL,
    hit_timestamp float NOT NULL
);