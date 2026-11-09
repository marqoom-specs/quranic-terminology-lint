"""One case per rule the linter enforces, and names it must leave alone."""
import pytest

from quranic_terminology_lint import rules

TERMS = rules.Terms()


def check(identifier, prose=False):
    return [(f["found"], f["preferred"], f["rules"], f["severity"])
            for f in rules.check(identifier, TERMS, prose)]


@pytest.mark.parametrize("identifier,found,preferred,rule", [
    ("chapter_title", "chapter", "surah", "001"),
    ("qaloun", "qaloun", "qalun", "056"),
    ("surat_id", "surat", "surah", "019"),
    ("aya_number", "aya", "ayah", "019"),
    ("hamzah_al_wasl", "hamzah_al_wasl", "hamzat_al_wasl", "020"),
    ("tajweed", "tajweed", "tajwid", "021"),
    ("tafseerText", "tafseer", "tafsir", "021"),
    ("makkiyy", "makkiyy", "makki", "022"),
    ("rub_al_hizb", "rub_al_hizb", "rubu_al_hizb", "026"),
    ("asbab_an_nuzul", "asbab_an_nuzul", "asbab_al_nuzul", "030"),
    ("al_fatihah", "al_fatihah", "fatihah", "031"),
    ("AlBaqarah", "al_baqarah", "baqarah", "031"),
    ("rasm_al_uthmani", "rasm_al_uthmani", "rasm_uthmani", "032"),
    ("rubu_hizb", "rubu_hizb", "rubu_al_hizb", "033"),
    ("waqf_jaiz", "waqf_jaiz", "waqf_jaiz_mustawi_al_tarafayn", "046"),
    ("omitted_alif", "omitted_alif", "dagger_alif", "046"),
    ("small_alif", "small_alif", "dagger_alif", "001"),
    ("ayat", "ayat", "ayahs", "049"),
    ("suwar", "suwar", "surahs", "049"),
    ("surah_no", "surah_no", "surah_number", "068"),
    ("sawtam", "sawtam", "phoneme", "001"),
    ("sifa", "sifa", "sifah", "019"),
    ("sifat", "sifat", "sifahs", "049"),
    ("silat_mim_al_jam", "silat_mim_al_jam", "silat_meem_al_jama", "001"),
    ("silat_meem_al_jam", "silat_meem_al_jam", "silat_meem_al_jama", "026"),
])
def test_errors(identifier, found, preferred, rule):
    assert (found, preferred, [rule], "error") in check(identifier)


@pytest.mark.parametrize("identifier,rule", [
    ("waqf_type", "050"),
    ("ayah_idx", "069"),
    ("hafs_word_timing", "073"),
    ("murattal_letter_timings", "073"),
    ("warsh_an_nafi_phoneme_timing", "073"),
    ("srh", "clear-names"),
    ("qirāʾah", "009"),
    ("qira'ah", "009"),
])
def test_warnings(identifier, rule):
    assert any(r == [rule] and s == "warning" for _, _, r, s in check(identifier))


def test_plural_keeps_number():
    assert check("suras") == [("suras", "surahs", ["019"], "error")]


@pytest.mark.parametrize("identifier", [
    "ayah", "surahs", "ayahKey", "SurahNumber", "WORD_TIMING", "word_timings", "has_word_timings",
    "word_timestamp", "word_timestamps", "WordTimestamp", "tajwid", "hamzat_al_wasl",
    "waqf_jaiz_mustawi_al_tarafayn", "waqfJaizMustawiAlTarafayn", "noon_sakinah", "small_meem",
    "aal_imran", "waqf_mark_type", "rubu_al_hizb", "asbab_al_nuzul", "makki", "kufi", "thumn_al_hizb",
    # The four levels of audio timing, and what the two finest are measured in.
    "ayah_timings", "letter_timings", "LetterTimings", "phoneme-timings", "getPhonemeTimings",
    "phonemes", "sifah", "sifahs", "silat_meem_al_jama", "madd_al_silah",
    # Two concepts carry `silah`, so alone it names neither; `qasr_silah` is a wajh.
    "silah", "qasr_silah",
    # Ordinary code must stay quiet.
    "part", "pause", "timing", "reading", "stop", "line_no", "token_type", "font_index",
    "feel", "room", "sad", "elephant", "data", "format", "root", "teen", "ay", "sabs", "noor",
])
def test_left_alone(identifier):
    assert check(identifier) == []


@pytest.mark.parametrize("identifier", ["al-Fatihah", "Qur'an", "Tajweed", "verse", "chapter", "ARABIC"])
def test_prose_allows_display_forms(identifier):
    assert check(identifier, prose=True) == []


def test_prose_still_rejects_misspellings():
    assert check("Sura", prose=True) == [("sura", "surah", ["019"], "error")]


def test_ignore_words():
    terms = rules.Terms(ignore_words=["sura"])
    assert rules.check("sura", terms) == [] and rules.check("suras", terms) == []
    assert rules.check("surah", terms) == []


@pytest.mark.parametrize("identifier", ["at_token", "atToken", "el_page", "alLine", "liine", "al_character"])
def test_english_concepts_do_not_receive_arabic_spelling_rules(identifier):
    assert check(identifier) == []


@pytest.mark.parametrize("identifier", ["ayaEnding", "suraObjectives", "ayaKey"])
def test_arabic_components_in_mixed_names_still_checked(identifier):
    assert any(severity == "error" for _, _, _, severity in check(identifier))


@pytest.mark.parametrize("identifier", ["aya", "ayas", "ayaKey", "aya_key", "AYA_KEY", "getAyaNo"])
def test_ignored_words_inside_identifiers(identifier):
    assert rules.check(identifier, rules.Terms(ignore_words=["aya"])) == []


def test_ignored_words_do_not_hide_other_mistakes():
    findings = rules.check("ayaKeySura", rules.Terms(ignore_words=["aya"]))
    assert [f["found"] for f in findings] == ["sura"]
    assert rules.check("ayat", rules.Terms(ignore_words=["aya"]))


def test_ignored_compound_respects_word_boundaries_and_case():
    terms = rules.Terms(ignore_words=["ayaKey"])
    assert rules.check("get_aya_key", terms) == []
    assert rules.check("ayaKeys", terms) == []
    assert rules.check("ayaNumber", terms)


def test_rules_predict_spellings_so_the_term_files_do_not_list_them():
    concepts = rules.read_table(rules.TERMS / "concepts.tsv")
    listed = {form for row in concepts for form in rules.items(row["other_spellings"])}
    assert not {"tajweed", "sura", "aya", "al_fatihah"} & listed


def test_term_files_are_consistent():
    codes = [row["code"] for row in rules.read_table(rules.TERMS / "concepts.tsv")]
    assert len(codes) == len(set(codes))
    for row in rules.read_table(rules.TERMS / "concepts.tsv"):
        assert row["origin"] in {"quranic", "borrowed", "standard"}, row["code"]
        assert row["code"] == rules.key(row["code"]) and row["code"].isascii()


def test_table_errors_name_the_line(tmp_path):
    (tmp_path / "concepts.tsv").write_text("code\tdisplay\norphan\n")
    with pytest.raises(ValueError, match="concepts.tsv:2"):
        rules.read_table(tmp_path / "concepts.tsv")


@pytest.mark.parametrize("text", ["the line's end", "don't", "it's", "we'll", "they're"])
def test_english_apostrophes_are_not_marks(text):
    assert all(check(identifier) == [] for _, identifier in rules.identifiers(text))
