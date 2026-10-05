# Campaign map scripts

The scripts that drew the first versions of the shipped campaigns'
scenarios (`assets/campaigns`). The RON files are the source now: edit
them by hand or in the scenario editor. Running a script again overwrites
its campaign's files.

```sh
python3 tools/campaigns/learning.py assets/campaigns/learning
python3 tools/campaigns/persian_wars.py assets/campaigns/persian-wars
python3 tools/campaigns/sargon.py assets/campaigns/sargon
python3 tools/campaigns/mapdraw.py /tmp assets/campaigns/sargon/*.ron  # top-down pictures (Pillow)
```
