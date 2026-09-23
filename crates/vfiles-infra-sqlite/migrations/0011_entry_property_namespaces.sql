-- Property identity is the namespace URI plus the local name. Existing rows
-- were returned in the DAV namespace, so keep that visible behavior on upgrade.
UPDATE entry_properties
SET prop_name = 'DAV:' || char(31) || prop_name
WHERE instr(prop_name, char(31)) = 0;
