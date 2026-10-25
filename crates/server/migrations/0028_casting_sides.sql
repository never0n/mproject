-- A blank's edge is stored per long side. An old single edge was ground on both sides.
CREATE FUNCTION pg_temp.sided(spans JSONB) RETURNS JSONB LANGUAGE SQL IMMUTABLE AS $$
    SELECT COALESCE(
        jsonb_agg(
            (span - 'edge_tenth_mm')
                || jsonb_build_object(
                    'edges_tenth_mm',
                    jsonb_build_array(
                        COALESCE(span -> 'edge_tenth_mm', '0'::jsonb),
                        COALESCE(span -> 'edge_tenth_mm', '0'::jsonb)
                    )
                )
            ORDER BY position
        ),
        '[]'::jsonb
    )
    FROM jsonb_array_elements(spans) WITH ORDINALITY AS cells(span, position)
$$;

UPDATE item_bodies
SET geometry = pg_temp.sided(geometry)
WHERE jsonb_typeof(geometry) = 'array';

UPDATE craft_sessions
SET checkpoint = jsonb_set(checkpoint, '{spans}', pg_temp.sided(checkpoint -> 'spans'))
WHERE jsonb_typeof(checkpoint -> 'spans') = 'array';
