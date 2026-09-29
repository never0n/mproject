-- Preserve the hierarchy while moving from the legacy eight roles to five.
ALTER TABLE accounts DROP CONSTRAINT accounts_role_check;
UPDATE accounts SET role = CASE WHEN role=1 THEN 1 WHEN role<=3 THEN 2 WHEN role<=5 THEN 3 WHEN role<=7 THEN 4 ELSE 5 END;
ALTER TABLE accounts ADD CONSTRAINT accounts_role_check CHECK(role BETWEEN 1 AND 5);
