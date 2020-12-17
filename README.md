![Screenshot](https://codeberg.org/treeshateorcs/bathory18/raw/branch/main/screenshot.png)

# bathory18

okay, so this is another rss reader from me. it works the same as lidya (and
lydia), same config file, same code, except that it does not have a ui. all it
does it display incoming articles through `notify-send`

one thing to note is that if you want to "read" all articles at once (without
the bother of clicking through tons of notifications), run it with the `-a` flag


# ~/.config/bathory18/urls sample
```
https://www.youtube.com/feeds/videos.xml?channel_id=UC_iD0xppBwwsrM9DegC5cQQ,Jon Gjengset,mpv
https://github.com/qutebrowser/qutebrowser/releases.atom,qb releases
#https://www.techrepublic.com/rssfeeds/articles/
```

# usage

-a to read all articles
bathory18 will automatically prevent running two instances
left click to open in browser (by default) or another program (as configured)
right click to dismiss
