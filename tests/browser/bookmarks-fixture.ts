import { mount } from 'svelte';
import BookmarksFixture from './BookmarksFixture.svelte';
import '../../src/styles.css';
mount(BookmarksFixture, { target: document.querySelector('main')! });
