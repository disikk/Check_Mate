INSERT INTO core.rooms (code, name)
VALUES ('gg', 'GG Poker')
ON CONFLICT (code) DO UPDATE
SET name = EXCLUDED.name;

INSERT INTO core.formats (room_id, code, name, max_players)
SELECT r.id, 'mbr', 'Mystery Battle Royale', 18
FROM core.rooms AS r
WHERE r.code = 'gg'
ON CONFLICT (room_id, code) DO UPDATE
SET
    name = EXCLUDED.name,
    max_players = EXCLUDED.max_players;
