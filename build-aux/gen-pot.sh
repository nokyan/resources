#!/bin/sh
shopt -s nullglob

rm po/resources.pot 2> /dev/null

xtr src/lib.rs -k i18n -k i18n_f -k i18n_k -k ni18n_f:1,2 -k ni18n:1,2 -k ni18n_k:1,2 -o po/rust.tmp.pot
xgettext data/resources/ui/**/*.ui -o po/ui.tmp.pot
xgettext data/resources/ui/*.ui -o po/ui_root.tmp.pot

mv data/net.nokyan.Resources.gschema.xml.in data/net.nokyan.Resources.gschema.xml.in.bak
xgettext data/*.in -o po/in.tmp.pot
mv data/net.nokyan.Resources.gschema.xml.in.bak data/net.nokyan.Resources.gschema.xml.in

sed -i 's/charset=CHARSET/charset=UTF-8/g' po/*.tmp.pot

xgettext po/*.tmp.pot -o po/resources.pot

msgmerge -N -U po/$1 po/resources.pot

rm po/*.tmp.pot